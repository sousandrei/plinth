use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    time::Duration,
};

use openraft::{
    Config, Raft,
    error::{InstallSnapshotError, RPCError, RaftError, RemoteError, Unreachable},
    network::{RPCOption, RaftNetwork, RaftNetworkFactory},
    raft::{
        AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest,
        InstallSnapshotResponse, VoteRequest, VoteResponse,
    },
};

use super::*;
use crate::membership::Role;

type Node = Raft<MembershipRaft>;
type RpcResult<T, E = openraft::error::Infallible> =
    Result<T, RPCError<u64, BasicNode, RaftError<u64, E>>>;

#[derive(Clone, Default)]
struct Network(Arc<Mutex<BTreeMap<u64, Node>>>);

struct Link {
    network: Network,
    target: u64,
}

impl Link {
    #[allow(clippy::result_large_err)]
    fn node<E: std::error::Error>(&self) -> RpcResult<Node, E> {
        self.network
            .0
            .lock()
            .unwrap()
            .get(&self.target)
            .cloned()
            .ok_or_else(|| {
                let error = io::Error::other("node is offline");
                RPCError::Unreachable(Unreachable::new(&error))
            })
    }
}

impl RaftNetworkFactory<MembershipRaft> for Network {
    type Network = Link;

    async fn new_client(&mut self, target: u64, _node: &BasicNode) -> Link {
        Link {
            network: self.clone(),
            target,
        }
    }
}

impl RaftNetwork<MembershipRaft> for Link {
    async fn append_entries(
        &mut self,
        request: AppendEntriesRequest<MembershipRaft>,
        _: RPCOption,
    ) -> RpcResult<AppendEntriesResponse<u64>> {
        self.node()?
            .append_entries(request)
            .await
            .map_err(|e| RemoteError::new(self.target, e).into())
    }

    async fn install_snapshot(
        &mut self,
        request: InstallSnapshotRequest<MembershipRaft>,
        _: RPCOption,
    ) -> RpcResult<InstallSnapshotResponse<u64>, InstallSnapshotError> {
        self.node()?
            .install_snapshot(request)
            .await
            .map_err(|e| RemoteError::new(self.target, e).into())
    }

    async fn vote(
        &mut self,
        request: VoteRequest<u64>,
        _: RPCOption,
    ) -> RpcResult<VoteResponse<u64>> {
        self.node()?
            .vote(request)
            .await
            .map_err(|e| RemoteError::new(self.target, e).into())
    }
}

#[tokio::test]
async fn surviving_majority_can_remove_the_creator() {
    let deadline = Duration::from_secs(8);
    let temp = tempfile::tempdir().unwrap();
    let network = Network::default();
    let cfg = Arc::new(
        Config {
            election_timeout_min: 250,
            election_timeout_max: 500,
            heartbeat_interval: 75,
            ..Config::default()
        }
        .validate()
        .unwrap(),
    );
    let mut nodes = BTreeMap::new();
    for id in 1..=3 {
        let store = MembershipLog::open(temp.path().join(format!("{id}.sqlite")), "space").unwrap();
        let node = Node::new(
            id,
            cfg.clone(),
            network.clone(),
            store.clone(),
            store.clone(),
        )
        .await
        .unwrap();
        network.0.lock().unwrap().insert(id, node.clone());
        nodes.insert(id, (node, store));
    }

    let (node_a, _) = &nodes[&1];
    node_a.initialize(BTreeSet::from([1])).await.unwrap();
    node_a
        .wait(Some(deadline))
        .current_leader(1, "initial leader")
        .await
        .unwrap();
    let first_device = [1; 32];
    let response = node_a
        .client_write(MembershipRequest::Initialize {
            owner_account: "account".into(),
            first_device,
            first_contact: None,
        })
        .await
        .unwrap();
    let owner = Enrollment {
        device: first_device,
        generation: response.log_id.index,
    };
    for id in [2, 3] {
        node_a
            .add_learner(id, BasicNode::default(), true)
            .await
            .unwrap();
        let outcome = node_a
            .client_write(MembershipRequest::Change {
                author: owner,
                change: Change::Enroll {
                    device: [id as u8; 32],
                    role: Role::Owner,
                },
            })
            .await
            .unwrap()
            .data;
        assert!(matches!(outcome, Outcome::Enrolled(_)));
    }
    let voters = node_a
        .change_membership(BTreeSet::from([1, 2, 3]), false)
        .await
        .unwrap()
        .log_id;
    for id in [2, 3] {
        nodes[&id]
            .0
            .wait(Some(deadline))
            .applied_index_at_least(Some(voters.index), "voters")
            .await
            .unwrap();
    }
    network.0.lock().unwrap().remove(&1);
    node_a.shutdown().await.unwrap();

    let leader = tokio::time::timeout(deadline, async {
        loop {
            if let Some(id @ (2 | 3)) = nodes[&2].0.current_leader().await {
                break id;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let (new_leader, store) = &nodes[&leader];
    let author = store
        .membership()
        .await
        .unwrap()
        .unwrap()
        .device([leader as u8; 32])
        .unwrap()
        .enrollment;
    let response = tokio::time::timeout(
        deadline,
        new_leader.client_write(MembershipRequest::Change {
            author,
            change: Change::Remove { enrollment: owner },
        }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.data, Outcome::Removed(owner));
    let (follower, store) = &nodes[&(5 - leader)];
    follower
        .wait(Some(deadline))
        .applied_index_at_least(Some(response.log_id.index), "committed removal")
        .await
        .unwrap();
    assert_eq!(
        store
            .membership()
            .await
            .unwrap()
            .unwrap()
            .removal_index(owner),
        Some(response.log_id.index)
    );

    network.0.lock().unwrap().clear();
    new_leader.shutdown().await.unwrap();
    follower.shutdown().await.unwrap();
}
