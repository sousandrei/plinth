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
    local: Enrollment,
    id: u64,
}

impl MembershipLog {
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
            network,
            log.clone(),
            log.clone(),
        )
        .await?;
        Ok(Self {
            log,
            node,
            local,
            id,
        })
    }

    /// Adds caught-up owners as voters in a single Raft membership change.
    /// Offer this once there are three owner devices to avoid creating a
    /// two-device quorum that fails when either device is offline.
    pub async fn promote_owners(&self) -> transport::Result<()> {
        let membership = self.active_membership().await?;
        let voters: BTreeSet<_> = membership
            .enrolled_owners()
            .map(|enrollment| voter_id(&membership, enrollment).ok_or("invalid owner enrollment"))
            .collect::<Result<_, _>>()?;
        if voters.len() < 3 {
            return Err(
                "at least three enrolled owner devices are required to promote voters".into(),
            );
        }
        let (_, configuration) = self.log.clone().applied_state().await?;
        if configuration.membership().get_joint_config().as_slice() == std::slice::from_ref(&voters)
        {
            return Ok(());
        }
        let current: BTreeSet<_> = configuration.voter_ids().collect();
        tokio::time::timeout(LOCAL_WRITE_TIMEOUT, async {
            for id in voters
                .difference(&current)
                .copied()
                .filter(|id| *id != self.id)
            {
                self.node
                    .add_learner(id, BasicNode::default(), true)
                    .await?;
            }
            self.node.change_membership(voters, false).await?;
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
        if membership.peer_contact(joining_device) != Some(&joining_contact) {
            self.node
                .wait(Some(LOCAL_WRITE_TIMEOUT))
                .current_leader(self.id, "pairing owner")
                .await?;
            let outcome = self
                .propose(Change::EnrollPeer {
                    contact: joining_contact,
                    role,
                })
                .await?;
            if !matches!(outcome, Outcome::Enrolled(_) | Outcome::ContactUpdated(_)) {
                return Err(format!("could not enroll device: {outcome:?}").into());
            }
        }
        self.log.clone().pairing_history().await
    }

    pub async fn respond(
        &self,
        mut stream: impl Read + Write + Send + 'static,
        certificate: &[u8],
    ) -> transport::Result<()> {
        let membership = self.active_membership().await?;
        let sender = owner(&membership, certificate_fingerprint(certificate))
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
        tokio::task::spawn_blocking(move || rpc::respond(&mut stream, sender, &node, &runtime))
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

fn owner(membership: &Membership, device: DeviceId) -> Option<Enrollment> {
    membership
        .device(device)
        .filter(|device| device.role == Role::Owner)
        .map(|device| device.enrollment)
}

fn voter_id(membership: &Membership, enrollment: Enrollment) -> Option<u64> {
    if membership.founding_enrollment() == Some(enrollment) {
        Some(FIRST_VOTER)
    } else {
        enrollment.generation.checked_add(1)
    }
}

#[cfg(test)]
#[path = "replica_tests.rs"]
mod tests;
