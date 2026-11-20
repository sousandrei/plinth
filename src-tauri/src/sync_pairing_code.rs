use std::io::{self, Read, Write};

use plinth_sync::transport::{read_frame, write_frame};
use ring::hmac;
use spake2::{Ed25519Group, Identity, Password, Spake2};

const MAX_EXCHANGE: usize = 64;
const JOINER: u8 = 0;
const HOST: u8 = 1;

pub(super) fn generate() -> Result<String, getrandom::Error> {
    const CODES: u32 = 1_000_000;
    const LIMIT: u32 = u32::MAX - u32::MAX % CODES;
    loop {
        let mut random = [0; 4];
        getrandom::fill(&mut random)?;
        let number = u32::from_be_bytes(random);
        if number < LIMIT {
            return Ok(format!("{:06}", number % CODES));
        }
    }
}

fn exchange(stream: &mut (impl Read + Write), code: &str, host: bool) -> io::Result<hmac::Key> {
    let password = Password::new(code.as_bytes());
    let joiner = Identity::new(b"plinth pairing joiner");
    let host_id = Identity::new(b"plinth pairing host");
    let (state, message) = if host {
        Spake2::<Ed25519Group>::start_b(&password, &joiner, &host_id)
    } else {
        Spake2::<Ed25519Group>::start_a(&password, &joiner, &host_id)
    };
    if host {
        let incoming = read_frame(stream, MAX_EXCHANGE)?;
        write_frame(stream, &message, MAX_EXCHANGE)?;
        finish(state, &incoming)
    } else {
        write_frame(stream, &message, MAX_EXCHANGE)?;
        finish(state, &read_frame(stream, MAX_EXCHANGE)?)
    }
}

fn finish(state: Spake2<Ed25519Group>, incoming: &[u8]) -> io::Result<hmac::Key> {
    let key = state
        .finish(incoming)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid pairing proof"))?;
    Ok(hmac::Key::new(hmac::HMAC_SHA256, &key))
}

// Include the TLS exporter so a confirmation applies to just this connection.
fn confirmation(binding: &[u8; 32], role: u8) -> [u8; 33] {
    let mut message = [role; 33];
    message[..32].copy_from_slice(binding);
    message
}

pub(super) fn host_proof(
    stream: &mut (impl Read + Write),
    code: &str,
    binding: &[u8; 32],
) -> io::Result<Option<hmac::Tag>> {
    let key = exchange(stream, code, true)?;
    let proof = read_frame(stream, MAX_EXCHANGE)?;
    Ok(hmac::verify(&key, &confirmation(binding, JOINER), &proof)
        .ok()
        .map(|()| hmac::sign(&key, &confirmation(binding, HOST))))
}

pub(super) fn verify(
    stream: &mut (impl Read + Write),
    code: &str,
    binding: &[u8; 32],
) -> io::Result<Verification> {
    let key = exchange(stream, code, false)?;
    let proof = hmac::sign(&key, &confirmation(binding, JOINER));
    write_frame(stream, proof.as_ref(), MAX_EXCHANGE)?;
    let reply = read_frame(stream, MAX_EXCHANGE)?;
    match reply.as_slice() {
        [] => return Ok(Verification::Incorrect),
        [0] => return Ok(Verification::Exhausted),
        _ => {}
    }
    hmac::verify(&key, &confirmation(binding, HOST), &reply)
        .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "pairing proof failed"))?;
    Ok(Verification::Accepted)
}

pub(super) enum Verification {
    Accepted,
    Incorrect,
    Exhausted,
}
