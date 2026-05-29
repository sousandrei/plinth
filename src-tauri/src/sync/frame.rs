use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::AppError;
use crate::sync::wire::Frame;

/// Hard ceiling on a single frame's payload. A change batch is the
/// largest expected frame; 16 MiB comfortably covers thousands of rows
/// while preventing a peer from forcing us to allocate unbounded memory.
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Length-prefixed write: 4-byte big-endian payload length, then the
/// postcard-encoded frame. The stream is *not* flushed — the caller
/// decides when to flush so a session can batch multiple frames into
/// one TLS record where it matters.
pub async fn write_frame<W>(stream: &mut W, frame: &Frame) -> Result<(), AppError>
where
    W: AsyncWrite + Unpin,
{
    let payload = postcard::to_allocvec(frame)
        .map_err(|e| AppError::Internal(format!("frame encode: {e}")))?;
    let len = u32::try_from(payload.len())
        .map_err(|_| AppError::Internal("frame too large".into()))?;
    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| AppError::Io(format!("frame write len: {e}")))?;
    stream
        .write_all(&payload)
        .await
        .map_err(|e| AppError::Io(format!("frame write body: {e}")))?;
    Ok(())
}

/// Length-prefixed read. Rejects frames larger than `MAX_FRAME_BYTES`
/// before allocating, so a malicious peer can't OOM us.
pub async fn read_frame<R>(stream: &mut R) -> Result<Frame, AppError>
where
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| AppError::Io(format!("frame read len: {e}")))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(AppError::Internal(format!(
            "frame too large: {len} bytes"
        )));
    }
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| AppError::Io(format!("frame read body: {e}")))?;
    postcard::from_bytes::<Frame>(&buf)
        .map_err(|e| AppError::Internal(format!("frame decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::wire::{Bye, Hello, PROTOCOL_VERSION};
    use tokio::io::duplex;

    #[tokio::test]
    async fn roundtrips_a_hello_frame() {
        let (mut a, mut b) = duplex(4096);
        let sent = Frame::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            device_id: "device-a".into(),
        });
        write_frame(&mut a, &sent).await.unwrap();
        a.flush().await.unwrap();
        drop(a);

        let got = read_frame(&mut b).await.unwrap();
        match got {
            Frame::Hello(h) => {
                assert_eq!(h.protocol_version, PROTOCOL_VERSION);
                assert_eq!(h.device_id, "device-a");
            }
            other => panic!("unexpected frame: {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_oversized_frame_header() {
        let (mut a, mut b) = duplex(8);
        // Write a length prefix that exceeds MAX_FRAME_BYTES, no body.
        let too_big = (MAX_FRAME_BYTES as u32 + 1).to_be_bytes();
        a.write_all(&too_big).await.unwrap();
        drop(a);

        let err = read_frame(&mut b).await.unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[tokio::test]
    async fn multiple_frames_back_to_back() {
        let (mut a, mut b) = duplex(4096);
        let f1 = Frame::Hello(Hello {
            protocol_version: 1,
            device_id: "x".into(),
        });
        let f2 = Frame::Bye(Bye {});
        write_frame(&mut a, &f1).await.unwrap();
        write_frame(&mut a, &f2).await.unwrap();
        a.flush().await.unwrap();
        drop(a);

        assert!(matches!(read_frame(&mut b).await.unwrap(), Frame::Hello(_)));
        assert!(matches!(read_frame(&mut b).await.unwrap(), Frame::Bye(_)));
    }
}
