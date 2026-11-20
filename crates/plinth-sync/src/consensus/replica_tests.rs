use std::{
    io,
    net::TcpListener,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use super::*;

struct TestDevice {
    identity: PathBuf,
    database: PathBuf,
    contact: PeerContact,
    listener: TcpListener,
}

impl TestDevice {
    fn new(root: &Path, name: &str) -> Self {
        let identity = root.join(name);
        Identity::init(&identity).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        Self {
            database: identity.join("sync.sqlite"),
            contact: PeerContact {
                certificate: std::fs::read(identity.join("peer.der")).unwrap(),
                address_hint: Some(listener.local_addr().unwrap()),
            },
            identity,
            listener,
        }
    }

    fn log(&self) -> MembershipLog {
        MembershipLog::open(&self.database, "space").unwrap()
    }

    async fn start(self, trusted: Vec<Vec<u8>>) -> (PermissionReplica, Listener) {
        let replica = PermissionReplica::open(self.log(), self.identity.clone(), |_| None)
            .await
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let runtime = tokio::runtime::Handle::current();
        self.listener.set_nonblocking(true).unwrap();
        let listener = thread::spawn({
            let replica = replica.clone();
            let stop = stop.clone();
            move || {
                let identity = Identity::with_certificates(
                    &self.identity,
                    trusted.into_iter().map(Into::into).collect(),
                )
                .unwrap();
                let listener = identity.listener_with_permissions().unwrap();
                while !stop.load(Ordering::Relaxed) {
                    match self.listener.accept() {
                        Ok((socket, _)) => {
                            socket.set_nonblocking(false).unwrap();
                            let _ = listener.accept_protocol(socket).and_then(
                                |(mut stream, certificate, protocol)| {
                                    assert_eq!(protocol, transport::PeerProtocol::Permissions);
                                    assert_eq!(transport::read_space(&mut stream)?, "space");
                                    runtime.block_on(replica.respond(stream, &certificate))
                                },
                            );
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("accept: {error}"),
                    }
                }
            }
        });
        (
            replica,
            Listener {
                stop,
                thread: Some(listener),
            },
        )
    }
}

struct Listener {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // Joining here can stall the async runtime handling an in-flight request.
            drop(thread);
        }
    }
}

#[tokio::test]
#[ignore = "requires localhost TCP"]
async fn enrolled_owners_form_a_permission_quorum_over_authenticated_tcp() {
    let dir = tempfile::tempdir().unwrap();
    let [host, offline, left, right] =
        ["host", "offline", "left", "right"].map(|name| TestDevice::new(dir.path(), name));
    let host_id = host.contact.device_id();
    let offline_id = offline.contact.device_id();
    initialize_local_space(
        host.database.clone(),
        host.identity.clone(),
        "space".into(),
        "account".into(),
        host.contact.clone(),
    )
    .await
    .unwrap();
    let mut host_log = host.log();
    for device in [&offline, &left, &right] {
        let history = host_log
            .approve_pairing(
                host.identity.clone(),
                "account",
                host_id,
                device.contact.clone(),
                Role::Owner,
            )
            .await
            .unwrap();
        device
            .log()
            .receive_pairing_history(
                history,
                host_id,
                device.contact.device_id(),
                Some("account"),
            )
            .await
            .unwrap();
    }
    drop(offline.listener);

    // The older replica learns the third owner through the existing roster exchange.
    let mut left_log = left.log();
    let rev = left_log
        .roster_revision(host_id, left.contact.device_id())
        .await
        .unwrap()
        .unwrap();
    let update = host_log
        .roster_update(host_id, left.contact.device_id(), rev)
        .await
        .unwrap()
        .unwrap();
    left_log
        .receive_roster_update(update, host_id, left.contact.device_id())
        .await
        .unwrap();

    let certs: Vec<_> = [&host, &left, &right]
        .map(|device| device.contact.certificate.clone())
        .into();
    let (a, server_a) = host.start(certs.clone()).await;
    let (b, _server_b) = left.start(certs.clone()).await;
    let (c, _server_c) = right.start(certs).await;
    let deadline = Duration::from_secs(10);
    a.node
        .wait(Some(deadline))
        .current_leader(a.id, "original owner")
        .await
        .unwrap();
    tokio::time::timeout(deadline, async {
        while !a.log.has_committed_shared_voters().await.unwrap() {
            a.promote_owners().await.unwrap();
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert!(!a.log.is_committed_voter(offline_id).await.unwrap());
    let (index, _) = a.log.clone().applied_state().await.unwrap();
    for follower in [&b, &c] {
        follower
            .node
            .wait(Some(deadline))
            .applied_index_at_least(index.map(|log| log.index), "final voter configuration")
            .await
            .unwrap();
    }
    a.shutdown().await.unwrap();
    drop(server_a);

    let promoted = tokio::time::timeout(deadline, async {
        loop {
            match b.node.current_leader().await {
                Some(id) if id == b.id => break &b,
                Some(id) if id == c.id => break &c,
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        promoted
            .propose(Change::Remove {
                enrollment: a.local
            })
            .await
            .unwrap(),
        Outcome::Removed(a.local)
    );
    let membership = promoted.log.membership().await.unwrap().unwrap();
    assert!(membership.permits_sync(b.local.device));
    assert!(!membership.permits_sync(a.local.device));
    assert_eq!(
        promoted.propose(Change::DeleteSpace).await.unwrap(),
        Outcome::SpaceDeleted
    );
    let (index, _) = promoted.log.clone().applied_state().await.unwrap();
    for replica in [&b, &c] {
        replica
            .node
            .wait(Some(deadline))
            .applied_index_at_least(index.map(|log| log.index), "committed deletion")
            .await
            .unwrap();
        assert!(
            replica
                .log
                .membership()
                .await
                .unwrap()
                .unwrap()
                .removal_index(a.local)
                .is_some()
        );
        assert_eq!(
            replica
                .log
                .membership()
                .await
                .unwrap()
                .unwrap()
                .deletion_index(),
            index.map(|log| log.index)
        );
        assert!(
            !replica
                .log
                .membership()
                .await
                .unwrap()
                .unwrap()
                .permits_sync(b.local.device)
        );
    }
    assert_eq!(
        promoted
            .propose(Change::Enroll {
                device: a.local.device,
                role: Role::Owner
            })
            .await
            .unwrap(),
        Outcome::SpaceAlreadyDeleted
    );
    b.shutdown().await.unwrap();
    c.shutdown().await.unwrap();
}
