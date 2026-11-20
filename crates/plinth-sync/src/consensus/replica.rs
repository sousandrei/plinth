use std::{
    io::{Read, Write},
    net::SocketAddr,
};

use crate::{
    membership::{Role, certificate_fingerprint},
    transport::Identity,
};

use super::*;

/// A running permission group member for an already enrolled owner device.
/// The caller owns its per-space lifecycle and authenticated listener.
#[derive(Clone)]
pub struct PermissionReplica {
    log: MembershipLog,
    node: Raft<MembershipRaft>,
    network: rpc::PermissionNetwork,
    local: Enrollment,
    id: u64,
}

impl MembershipLog {
    pub async fn has_committed_shared_voters(&self) -> transport::Result<bool> {
        let state = self.pairing_state().await?;
        Ok(is_shared_configuration(&state.voters))
    }

    pub async fn should_resume_replica(&self, device: DeviceId) -> transport::Result<bool> {
        let state = self.pairing_state().await?;
        let Some(membership) = state.membership else {
            return Ok(false);
        };
        let Some(id) =
            owner(&membership, device).and_then(|enrollment| voter_id(&membership, enrollment))
        else {
            return Ok(false);
        };
        Ok((id == FIRST_VOTER
            && membership.deletion_index().is_none()
            && state.voters.voter_ids().eq([FIRST_VOTER])
            && has_owner_quorum(&membership))
            || (state.voters.nodes().count() > 1
                && state.voters.nodes().any(|(node, _)| *node == id)))
    }

    pub async fn is_committed_voter(&self, device: DeviceId) -> transport::Result<bool> {
        let state = self.pairing_state().await?;
        let Some(membership) = state.membership else {
            return Ok(false);
        };
        let Some(id) =
            owner(&membership, device).and_then(|enrollment| voter_id(&membership, enrollment))
        else {
            return Ok(false);
        };
        Ok(state.voters.voter_ids().any(|voter| voter == id))
    }
}

impl PermissionReplica {
    pub async fn shared_roster(&self) -> transport::Result<Option<Membership>> {
        let state = self.log.pairing_state().await?;
        Ok(state.membership.filter(|membership| {
            is_shared_configuration(&state.voters) && membership.permits_sync(self.local.device)
        }))
    }

    pub async fn open(
        log: MembershipLog,
        identity_dir: PathBuf,
        resolve: impl Fn(&PeerContact) -> Option<SocketAddr> + Send + Sync + 'static,
    ) -> transport::Result<Self> {
        let path = identity_dir.clone();
        let device = tokio::task::spawn_blocking(move || {
            Identity::with_certificates(&path, Vec::new())
                .map(|identity| certificate_fingerprint(identity.certificate()))
        })
        .await??;
        Self::open_for_device(log, identity_dir, device, resolve).await
    }

    pub(super) async fn open_for_device(
        log: MembershipLog,
        identity_dir: PathBuf,
        device: DeviceId,
        resolve: impl Fn(&PeerContact) -> Option<SocketAddr> + Send + Sync + 'static,
    ) -> transport::Result<Self> {
        let membership = log
            .membership()
            .await?
            .ok_or("missing permission history")?;
        let local = owner(&membership, device).ok_or("this device is not an enrolled owner")?;
        let id = voter_id(&membership, local).ok_or("invalid owner enrollment")?;
        let network =
            rpc::PermissionNetwork::with_async_lookup(identity_dir, log.space_id.clone(), {
                let log = log.clone();
                let resolve = Arc::new(resolve);
                move |target| {
                    let log = log.clone();
                    let resolve = resolve.clone();
                    async move {
                        let membership = log.membership().await.ok().flatten()?;
                        let device = membership.enrolled_owners().find(|enrollment| {
                            voter_id(&membership, *enrollment) == Some(target)
                        })?;
                        let contact = membership.peer_contact(device.device)?;
                        Some(rpc::VoterEndpoint {
                            address: resolve(contact).or(contact.address_hint)?,
                            certificate: contact.certificate.clone(),
                        })
                    }
                }
            });
        let node = Raft::new(
            id,
            Arc::new(Config::default()),
            network.clone(),
            log.clone(),
            log.clone(),
        )
        .await?;
        Ok(Self {
            log,
            node,
            network,
            local,
            id,
        })
    }

    /// Bootstrap a three-owner quorum once two other owners have caught up.
    pub async fn promote_owners(&self) -> transport::Result<()> {
        if self.node.current_leader().await != Some(self.id) {
            return Ok(());
        }
        tokio::time::timeout(LOCAL_WRITE_TIMEOUT, async {
            let state = self.log.pairing_state().await?;
            let Some(membership) = state.membership.filter(|membership| {
                membership.deletion_index().is_none()
                    && owner(membership, self.local.device) == Some(self.local)
            }) else {
                return Ok(());
            };
            let voters = match state.voters.membership().get_joint_config().as_slice() {
                [initial, selected] if initial.iter().copied().eq([FIRST_VOTER]) => {
                    selected.clone()
                }
                [initial]
                    if initial.iter().copied().eq([FIRST_VOTER])
                        && has_owner_quorum(&membership) =>
                {
                    let candidates: BTreeSet<_> = membership
                        .enrolled_owners()
                        .filter(|enrollment| enrollment.device != self.local.device)
                        .filter(|enrollment| membership.peer_contact(enrollment.device).is_some())
                        .filter_map(|enrollment| voter_id(&membership, enrollment))
                        .collect();
                    for &id in &candidates {
                        if state.voters.membership().get_node(&id).is_none() {
                            self.node
                                .add_learner(id, BasicNode::default(), false)
                                .await?;
                        }
                    }
                    let state = self.log.pairing_state().await?;
                    let metrics = self.node.metrics();
                    let ready: BTreeSet<_> = metrics
                        .borrow()
                        .replication
                        .iter()
                        .flat_map(|replication| {
                            candidates.iter().copied().filter(move |id| {
                                replication.get(id).copied().flatten() >= state.last_applied
                            })
                        })
                        .take(2)
                        .chain([FIRST_VOTER])
                        .collect();
                    if ready.len() < 3 {
                        return Ok(());
                    }
                    ready
                }
                _ => return Ok(()),
            };
            self.node.change_membership(voters, true).await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await??;
        Ok(())
    }

    pub async fn propose(&self, change: Change) -> transport::Result<Outcome> {
        self.active_membership().await?;
        let response = tokio::time::timeout(
            LOCAL_WRITE_TIMEOUT,
            self.node.client_write(MembershipRequest::Change {
                author: self.local,
                change,
            }),
        )
        .await??;
        Ok(response.data)
    }

    pub async fn approve_pairing(
        &self,
        owner_account: &str,
        joining_contact: PeerContact,
        role: Role,
    ) -> transport::Result<PairingHistory> {
        if !self
            .log
            .owner_can_pair(owner_account, self.local.device)
            .await?
        {
            return Err("missing permission history".into());
        }
        let membership = self.active_membership().await?;
        let joining_device = joining_contact.device_id();
        if joining_device == self.local.device {
            return Err("cannot pair a device with itself".into());
        }
        if membership
            .device(joining_device)
            .is_some_and(|device| device.role != role)
        {
            return Err("this device already has a different role in this space".into());
        }
        let outcome = self
            .commit_device_change(MembershipRequest::Change {
                author: self.local,
                change: Change::EnrollPeer {
                    contact: joining_contact.clone(),
                    role,
                },
            })
            .await?;
        if !matches!(
            outcome,
            Outcome::Enrolled(_) | Outcome::ContactUpdated(_) | Outcome::AlreadyEnrolled
        ) {
            return Err(format!("could not enroll device: {outcome:?}").into());
        }
        let updated = self.active_membership().await?;
        if !updated
            .device(joining_device)
            .is_some_and(|device| device.role == role)
            || updated.peer_contact(joining_device) != Some(&joining_contact)
        {
            return Err("the approved device enrollment changed".into());
        }
        self.log.clone().pairing_history().await
    }

    pub async fn revoke_device(
        &self,
        owner_account: &str,
        enrollment: Enrollment,
    ) -> transport::Result<Outcome> {
        let membership = self.active_membership().await?;
        if membership.owner_account() != owner_account
            || !self.log.is_committed_voter(self.local.device).await?
        {
            return Err("remove devices from a voting owner device in this space".into());
        }
        if self.local.device == enrollment.device {
            return Err("cannot remove this device from itself".into());
        }
        self.commit_device_change(MembershipRequest::RevokeDevice {
            author: self.local,
            enrollment,
        })
        .await
    }

    async fn commit_device_change(&self, request: MembershipRequest) -> transport::Result<Outcome> {
        tokio::time::timeout(LOCAL_WRITE_TIMEOUT, async {
            let (index, outcome) = loop {
                if let Some(leader) = self.node.current_leader().await {
                    let committed = if leader == self.id {
                        self.node
                            .client_write(request.clone())
                            .await
                            .ok()
                            .map(|response| (response.log_id.index, response.data))
                    } else {
                        self.network
                            .forward_change(leader, request.clone())
                            .await
                            .ok()
                            .flatten()
                    };
                    if let Some(committed) = committed {
                        break committed;
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            };
            self.node
                .wait(Some(LOCAL_WRITE_TIMEOUT))
                .applied_index_at_least(Some(index), "committed device change")
                .await?;
            Ok(outcome)
        })
        .await
        .map_err(|_| "could not confirm the change with the space's voting devices")?
    }

    pub async fn respond(
        &self,
        mut stream: impl Read + Write + Send + 'static,
        certificate: &[u8],
    ) -> transport::Result<()> {
        let membership = self.active_membership().await?;
        let enrollment = owner(&membership, certificate_fingerprint(certificate));
        let sender = enrollment
            .and_then(|enrollment| voter_id(&membership, enrollment))
            .filter(|id| {
                self.node
                    .metrics()
                    .borrow()
                    .membership_config
                    .voter_ids()
                    .any(|voter| voter == *id)
            })
            .ok_or("permission RPC sender is not an active voter")?;
        let node = self.node.clone();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            rpc::respond(&mut stream, sender, enrollment, &node, &runtime)
        })
        .await?
    }

    pub async fn shutdown(&self) -> transport::Result<()> {
        self.node.shutdown().await?;
        Ok(())
    }

    async fn active_membership(&self) -> transport::Result<Membership> {
        let membership = self
            .log
            .membership()
            .await?
            .ok_or("missing permission history")?;
        if owner(&membership, self.local.device) != Some(self.local) {
            return Err("this owner enrollment is no longer active".into());
        }
        Ok(membership)
    }
}

fn is_shared_configuration(configuration: &Configuration) -> bool {
    matches!(configuration.membership().get_joint_config().as_slice(), [voters] if voters.len() > 1)
}

fn has_owner_quorum(membership: &Membership) -> bool {
    membership
        .enrolled_owners()
        .filter(|enrollment| Some(*enrollment) != membership.founding_enrollment())
        .filter(|enrollment| membership.peer_contact(enrollment.device).is_some())
        .take(2)
        .count()
        == 2
}

pub(super) fn owner(membership: &Membership, device: DeviceId) -> Option<Enrollment> {
    membership
        .device(device)
        .filter(|device| device.role == Role::Owner)
        .map(|device| device.enrollment)
}

pub(super) fn voter_id(membership: &Membership, enrollment: Enrollment) -> Option<u64> {
    if membership.founding_enrollment() == Some(enrollment) {
        Some(FIRST_VOTER)
    } else {
        enrollment.generation.checked_add(1)
    }
}

#[cfg(test)]
#[path = "replica_tests.rs"]
mod tests;
