//! Mutually authenticated, per-space Raft RPCs.

use std::{
    future::{Future, ready},
    io::{self, Read, Write},
    net::SocketAddr,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use openraft::{
    BasicNode, Raft, Vote,
    error::{InstallSnapshotError, RPCError, RaftError, RemoteError, Unreachable},
    network::{RPCOption, RaftNetwork, RaftNetworkFactory},
    raft::{
        AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest,
        InstallSnapshotResponse, VoteRequest, VoteResponse,
    },
};
use serde::{Deserialize, Serialize};

use crate::{
    membership::{Change, Enrollment, Outcome, PeerContact, Role},
    transport::{self, Identity, PeerProtocol},
};

use super::{LOCAL_WRITE_TIMEOUT, MembershipRaft, MembershipRequest};

// A default OpenRaft snapshot chunk is 3 MiB before JSON framing.
const MAX_RPC_BYTES: usize = 16 * 1024 * 1024;
const MAX_SPACE_BYTES: usize = 256;

type RpcResult<T, E = openraft::error::Infallible> =
    Result<T, RPCError<u64, BasicNode, RaftError<u64, E>>>;

#[derive(Serialize, Deserialize)]
enum Request {
    Append(AppendEntriesRequest<MembershipRaft>),
    Snapshot(InstallSnapshotRequest<MembershipRaft>),
    Vote(VoteRequest<u64>),
    Enroll {
        generation: u64,
        contact: PeerContact,
        role: Role,
    },
    Remove {
        generation: u64,
        enrollment: Enrollment,
    },
    DeleteSpace {
        generation: u64,
    },
}

impl Request {
    fn vote(&self) -> Option<&Vote<u64>> {
        match self {
            Self::Append(request) => Some(&request.vote),
            Self::Snapshot(request) => Some(&request.vote),
            Self::Vote(request) => Some(&request.vote),
            Self::Enroll { .. } | Self::Remove { .. } | Self::DeleteSpace { .. } => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
enum Response {
    Append(Result<AppendEntriesResponse<u64>, RaftError<u64>>),
    Snapshot(Result<InstallSnapshotResponse<u64>, RaftError<u64, InstallSnapshotError>>),
    Vote(Result<VoteResponse<u64>, RaftError<u64>>),
    #[serde(rename = "Enrollment")]
    Applied(Option<(u64, Outcome)>),
}

pub struct VoterEndpoint {
    pub address: SocketAddr,
    pub certificate: Vec<u8>,
}

type EndpointLookup = Pin<Box<dyn Future<Output = Option<VoterEndpoint>> + Send>>;

/// Resolves addresses independently from authorization. The certificate must
/// come from the space's trusted voter directory, never from discovery.
#[derive(Clone)]
pub struct PermissionNetwork {
    identity_dir: Arc<PathBuf>,
    space_id: Arc<str>,
    endpoint: Arc<dyn Fn(u64) -> EndpointLookup + Send + Sync>,
}

impl PermissionNetwork {
    pub(super) async fn forward_change(
        &self,
        target: u64,
        change: MembershipRequest,
    ) -> transport::Result<Option<(u64, Outcome)>> {
        let request = match change {
            MembershipRequest::Change {
                author,
                change: Change::EnrollPeer { contact, role },
            } => Request::Enroll {
                generation: author.generation,
                contact,
                role,
            },
            MembershipRequest::RevokeDevice { author, enrollment } => Request::Remove {
                generation: author.generation,
                enrollment,
            },
            MembershipRequest::Change {
                author,
                change: Change::DeleteSpace,
            } => Request::DeleteSpace {
                generation: author.generation,
            },
            _ => return Err("unsupported forwarded permission change".into()),
        };
        let connection = PermissionConnection {
            network: self.clone(),
            target,
        };
        match connection.exchange(request, LOCAL_WRITE_TIMEOUT).await? {
            Response::Applied(outcome) => Ok(outcome),
            _ => Err("unexpected permission RPC response".into()),
        }
    }

    pub fn new(
        identity_dir: PathBuf,
        space_id: impl Into<Arc<str>>,
        endpoint: impl Fn(u64) -> Option<VoterEndpoint> + Send + Sync + 'static,
    ) -> Self {
        Self::with_async_lookup(identity_dir, space_id, move |target| {
            ready(endpoint(target))
        })
    }

    pub(crate) fn with_async_lookup<F>(
        identity_dir: PathBuf,
        space_id: impl Into<Arc<str>>,
        endpoint: impl Fn(u64) -> F + Send + Sync + 'static,
    ) -> Self
    where
        F: Future<Output = Option<VoterEndpoint>> + Send + 'static,
    {
        Self {
            identity_dir: Arc::new(identity_dir),
            space_id: space_id.into(),
            endpoint: Arc::new(move |target| Box::pin(endpoint(target))),
        }
    }
}

pub struct PermissionConnection {
    network: PermissionNetwork,
    target: u64,
}

impl PermissionConnection {
    async fn exchange(&self, request: Request, timeout: Duration) -> transport::Result<Response> {
        let network = self.network.clone();
        let target = self.target;
        let endpoint = (network.endpoint)(target)
            .await
            .ok_or("missing voter endpoint")?;
        tokio::task::spawn_blocking(move || {
            let identity = Identity::with_certificates(
                &network.identity_dir,
                vec![endpoint.certificate.into()],
            )?;
            let mut stream =
                identity.connect_protocol(endpoint.address, PeerProtocol::Permissions, timeout)?;
            transport::write_frame(&mut stream, network.space_id.as_bytes(), MAX_SPACE_BYTES)?;
            let bytes = serde_json::to_vec(&request)?;
            transport::write_frame(&mut stream, &bytes, MAX_RPC_BYTES)?;
            let bytes = transport::read_frame(&mut stream, MAX_RPC_BYTES)?;
            Ok(serde_json::from_slice(&bytes)?)
        })
        .await?
    }
}

impl RaftNetworkFactory<MembershipRaft> for PermissionNetwork {
    type Network = PermissionConnection;

    async fn new_client(&mut self, target: u64, _node: &BasicNode) -> Self::Network {
        PermissionConnection {
            network: self.clone(),
            target,
        }
    }
}

fn unreachable(message: impl ToString) -> Unreachable {
    Unreachable::new(&io::Error::other(message.to_string()))
}

impl RaftNetwork<MembershipRaft> for PermissionConnection {
    async fn append_entries(
        &mut self,
        request: AppendEntriesRequest<MembershipRaft>,
        option: RPCOption,
    ) -> RpcResult<AppendEntriesResponse<u64>> {
        match self
            .exchange(Request::Append(request), option.hard_ttl())
            .await
            .map_err(unreachable)?
        {
            Response::Append(response) => {
                response.map_err(|error| RemoteError::new(self.target, error).into())
            }
            _ => Err(unreachable("unexpected permission RPC response").into()),
        }
    }

    async fn install_snapshot(
        &mut self,
        request: InstallSnapshotRequest<MembershipRaft>,
        option: RPCOption,
    ) -> RpcResult<InstallSnapshotResponse<u64>, InstallSnapshotError> {
        match self
            .exchange(Request::Snapshot(request), option.hard_ttl())
            .await
            .map_err(unreachable)?
        {
            Response::Snapshot(response) => {
                response.map_err(|error| RemoteError::new(self.target, error).into())
            }
            _ => Err(unreachable("unexpected permission RPC response").into()),
        }
    }

    async fn vote(
        &mut self,
        request: VoteRequest<u64>,
        option: RPCOption,
    ) -> RpcResult<VoteResponse<u64>> {
        match self
            .exchange(Request::Vote(request), option.hard_ttl())
            .await
            .map_err(unreachable)?
        {
            Response::Vote(response) => {
                response.map_err(|error| RemoteError::new(self.target, error).into())
            }
            _ => Err(unreachable("unexpected permission RPC response").into()),
        }
    }
}

/// Handle one RPC after TLS negotiation and reading the bounded space header.
/// The caller must derive the sender's voter ID from the client certificate
/// using that space's trusted voter configuration.
pub fn respond(
    stream: &mut (impl Read + Write),
    authenticated_voter: u64,
    authenticated_enrollment: Option<Enrollment>,
    raft: &Raft<MembershipRaft>,
    runtime: &tokio::runtime::Handle,
) -> transport::Result<()> {
    let request: Request = serde_json::from_slice(&transport::read_frame(stream, MAX_RPC_BYTES)?)?;
    if request
        .vote()
        .is_some_and(|vote| vote.leader_id().node_id != authenticated_voter)
    {
        return Err("permission RPC voter does not match the authenticated device".into());
    }
    let response = runtime.block_on(async {
        let response = match request {
            Request::Append(request) => Response::Append(raft.append_entries(request).await),
            Request::Snapshot(request) => Response::Snapshot(raft.install_snapshot(request).await),
            Request::Vote(request) => Response::Vote(raft.vote(request).await),
            Request::Enroll {
                generation,
                contact,
                role,
            } => {
                let author = authenticated_enrollment
                    .filter(|author| {
                        author.generation == generation && author.device != contact.device_id()
                    })
                    .ok_or("pairing request does not match the authenticated enrollment")?;
                if contact.certificate.is_empty()
                    || contact.certificate.len() > transport::MAX_ENROLLMENT_REQUEST
                {
                    return Err("invalid pairing certificate".into());
                }
                Response::Applied(
                    commit_forwarded(
                        raft,
                        MembershipRequest::Change {
                            author,
                            change: Change::EnrollPeer { contact, role },
                        },
                    )
                    .await,
                )
            }
            Request::Remove {
                generation,
                enrollment,
            } => {
                let author = authenticated_enrollment
                    .filter(|author| {
                        author.generation == generation && author.device != enrollment.device
                    })
                    .ok_or("removal request does not match the authenticated enrollment")?;
                Response::Applied(
                    commit_forwarded(raft, MembershipRequest::RevokeDevice { author, enrollment })
                        .await,
                )
            }
            Request::DeleteSpace { generation } => {
                let author = authenticated_enrollment
                    .filter(|author| author.generation == generation)
                    .ok_or("space deletion does not match the authenticated enrollment")?;
                Response::Applied(
                    commit_forwarded(
                        raft,
                        MembershipRequest::Change {
                            author,
                            change: Change::DeleteSpace,
                        },
                    )
                    .await,
                )
            }
        };
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(response)
    })?;
    transport::write_frame(stream, &serde_json::to_vec(&response)?, MAX_RPC_BYTES)?;
    Ok(())
}

async fn commit_forwarded(
    raft: &Raft<MembershipRaft>,
    request: MembershipRequest,
) -> Option<(u64, Outcome)> {
    tokio::time::timeout(LOCAL_WRITE_TIMEOUT, raft.client_write(request))
        .await
        .ok()
        .and_then(Result::ok)
        .map(|response| (response.log_id.index, response.data))
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use openraft::{Config, storage::RaftLogStorage};

    use super::*;
    use crate::consensus::MembershipLog;

    #[tokio::test]
    #[ignore = "requires localhost TCP"]
    async fn authenticates_voter_identity_and_preserves_data_connections() {
        let temp = tempfile::tempdir().unwrap();
        let client_dir = temp.path().join("client");
        let server_dir = temp.path().join("server");
        Identity::init(&client_dir).unwrap();
        Identity::init(&server_dir).unwrap();
        let client_cert = std::fs::read(client_dir.join("peer.der")).unwrap();
        let server_cert = std::fs::read(server_dir.join("peer.der")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut store = MembershipLog::open(temp.path().join("sync.sqlite"), "space").unwrap();
        let raft = Raft::new(
            2,
            Arc::new(Config::default()),
            PermissionNetwork::new(server_dir.clone(), "space", |_| None),
            store.clone(),
            store.clone(),
        )
        .await
        .unwrap();
        let runtime = tokio::runtime::Handle::current();
        let server = tokio::task::spawn_blocking({
            let raft = raft.clone();
            move || {
                let identity =
                    Identity::with_certificates(&server_dir, vec![client_cert.clone().into()])
                        .unwrap();
                let accept = identity.desktop_listener().unwrap();
                for id in [1, 9] {
                    let (socket, _) = listener.accept().unwrap();
                    let (mut stream, origin, protocol) = accept.accept_protocol(socket).unwrap();
                    assert_eq!(origin, client_cert);
                    assert_eq!(protocol, PeerProtocol::Permissions);
                    assert_eq!(transport::read_space(&mut stream).unwrap(), "space");
                    assert_eq!(
                        respond(&mut stream, 1, None, &raft, &runtime).is_ok(),
                        id == 1
                    );
                }
                let (socket, _) = listener.accept().unwrap();
                let (_, origin) = accept.accept_identified(socket).unwrap();
                assert_eq!(origin, client_cert);
            }
        });
        let mut network = PermissionNetwork::new(client_dir.clone(), "space", {
            let server_cert = server_cert.clone();
            move |_| {
                Some(VoterEndpoint {
                    address,
                    certificate: server_cert.clone(),
                })
            }
        });
        let connection = network.new_client(2, &BasicNode::default()).await;
        let timeout = Duration::from_secs(5);
        assert!(matches!(
            connection
                .exchange(
                    Request::Vote(VoteRequest {
                        vote: Vote::new(1, 1),
                        last_log_id: None
                    }),
                    timeout
                )
                .await
                .unwrap(),
            Response::Vote(Ok(_))
        ));
        assert!(
            connection
                .exchange(
                    Request::Vote(VoteRequest {
                        vote: Vote::new(2, 9),
                        last_log_id: None
                    }),
                    timeout
                )
                .await
                .is_err()
        );
        assert_eq!(store.read_vote().await.unwrap(), Some(Vote::new(1, 1)));

        tokio::task::spawn_blocking(move || {
            let identity =
                Identity::with_certificates(&client_dir, vec![server_cert.into()]).unwrap();
            identity.connect(address).unwrap();
        })
        .await
        .unwrap();
        server.await.unwrap();
        raft.shutdown().await.unwrap();
    }
}
