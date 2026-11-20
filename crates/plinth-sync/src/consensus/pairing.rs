use super::replica::{owner, voter_id};
use super::*;
use crate::membership::Role;

/// A committed permission snapshot sent over a confirmed pairing connection.
#[derive(Serialize, Deserialize)]
pub struct PairingHistory(StoredSnapshot);

impl MembershipLog {
    pub async fn owner_can_pair(
        &self,
        owner_account: &str,
        local_device: DeviceId,
    ) -> transport::Result<bool> {
        let state = self.pairing_state().await?;
        let Some(membership) = state.membership else {
            return if state.last_applied.is_none() {
                Ok(false)
            } else {
                Err("space permission history is incomplete".into())
            };
        };
        if membership.deletion_index().is_some() {
            return Err("this space was deleted".into());
        }
        if membership.owner_account() != owner_account
            || !is_pairing_owner(&membership, &state.voters, local_device)
        {
            return Err("pair devices from a voting owner device in this space".into());
        }
        Ok(true)
    }

    /// Commit the approved key before handing the resulting history to the
    /// joiner. Callers serialize this with other local Raft writers.
    pub async fn approve_pairing(
        &mut self,
        identity_dir: PathBuf,
        owner_account: &str,
        local_device: DeviceId,
        joining_contact: PeerContact,
        role: Role,
    ) -> transport::Result<Option<PairingHistory>> {
        if !self.owner_can_pair(owner_account, local_device).await? {
            return Ok(None);
        }
        let replica =
            PermissionReplica::open_for_device(self.clone(), identity_dir, local_device, |_| None)
                .await?;
        let approved = replica
            .approve_pairing(owner_account, joining_contact, role)
            .await;
        let stopped = replica.shutdown().await;
        let history = approved?;
        stopped?;
        Ok(Some(history))
    }

    pub(super) async fn pairing_history(&mut self) -> transport::Result<PairingHistory> {
        let snapshot = self.build_snapshot().await?;
        Ok(PairingHistory(StoredSnapshot {
            meta: snapshot.meta,
            contents: serde_json::from_slice(snapshot.snapshot.get_ref())?,
        }))
    }

    pub async fn roster_revision(
        &self,
        host: DeviceId,
        local_device: DeviceId,
    ) -> transport::Result<Option<u64>> {
        let state = self.pairing_state().await?;
        Ok(state
            .membership
            .as_ref()
            .filter(|membership| {
                host != local_device
                    && state.voters.voter_ids().eq([FIRST_VOTER])
                    && is_founder(membership, host)
                    && membership.device(local_device).is_some()
            })
            .and(state.last_applied)
            .map(|log| log.index))
    }

    pub async fn roster_update(
        &self,
        local_device: DeviceId,
        requester: DeviceId,
        known_revision: u64,
    ) -> transport::Result<Option<PairingHistory>> {
        let state = self.pairing_state().await?;
        let membership = state
            .membership
            .as_ref()
            .ok_or("missing permission history")?;
        if !state.voters.voter_ids().eq([FIRST_VOTER])
            || !is_founder(membership, local_device)
            || membership.device(requester).is_none()
        {
            return Err("device is not authorized for this roster".into());
        }
        if state
            .last_applied
            .is_none_or(|log| log.index <= known_revision)
        {
            return Ok(None);
        }
        // Pairing saves the committed snapshot for the latest roster. A request
        // during an approval can pick it up on the next exchange.
        Ok(self
            .load_snapshot()
            .await?
            .filter(|snapshot| {
                snapshot
                    .meta
                    .last_log_id
                    .is_some_and(|log| log.index > known_revision)
            })
            .map(PairingHistory))
    }

    pub async fn receive_roster_update(
        &mut self,
        history: PairingHistory,
        host: DeviceId,
        local_device: DeviceId,
    ) -> transport::Result<()> {
        let current = self.pairing_state().await?;
        let membership = current
            .membership
            .as_ref()
            .ok_or("missing permission history")?;
        let device = membership
            .device(local_device)
            .ok_or("device is not enrolled")?;
        let incoming = history
            .0
            .contents
            .state
            .membership
            .as_ref()
            .ok_or("missing permission history")?;
        if host == local_device
            || !current.voters.voter_ids().eq([FIRST_VOTER])
            || !is_founder(membership, host)
            || incoming.device(local_device) != Some(device)
            || history.0.meta.last_log_id.map(|log| log.index)
                <= current.last_applied.map(|log| log.index)
        {
            return Err("roster update does not match the current enrollment".into());
        }
        let account = (device.role == Role::Owner).then_some(membership.owner_account());
        self.receive_pairing_history(Some(history), host, local_device, account)
            .await
    }

    #[allow(clippy::result_large_err)]
    async fn load_snapshot(&self) -> RaftResult<Option<StoredSnapshot>> {
        self.access(
            ErrorSubject::Snapshot(None),
            ErrorVerb::Read,
            |db, space| read_meta(db, space, "snapshot"),
        )
        .await
    }

    pub async fn receive_pairing_history(
        &mut self,
        history: Option<PairingHistory>,
        host: DeviceId,
        local_device: DeviceId,
        shared_account: Option<&str>,
    ) -> transport::Result<()> {
        let current = self.membership().await?;
        let Some(PairingHistory(snapshot)) = history else {
            return if current.is_none() {
                Ok(())
            } else {
                Err("pairing omitted the existing permission history".into())
            };
        };
        let incoming = snapshot
            .contents
            .state
            .membership
            .as_ref()
            .ok_or("missing permission history")?;
        let expected_role = if let Some(account) = shared_account {
            if account != incoming.owner_account() {
                return Err("shared account does not match the space owner".into());
            }
            Role::Owner
        } else {
            Role::Member
        };
        if host == local_device
            || !is_pairing_owner(incoming, &snapshot.meta.last_membership, host)
            || incoming.deletion_index().is_some()
            || !incoming
                .device(local_device)
                .is_some_and(|device| device.role == expected_role)
            || current.as_ref().is_some_and(|existing| {
                existing.owner_account() != incoming.owner_account()
                    || existing.founding_enrollment() != incoming.founding_enrollment()
            })
        {
            return Err("permission history does not match the approved pairing".into());
        }
        let snapshot = snapshot.into_snapshot()?;
        self.install_snapshot(&snapshot.meta, snapshot.snapshot)
            .await?;
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    pub(super) async fn pairing_state(&self) -> RaftResult<AppliedState> {
        self.access(ErrorSubject::StateMachine, ErrorVerb::Read, |db, space| {
            read_state(db, space)
        })
        .await
    }
}

fn is_founder(membership: &Membership, device: DeviceId) -> bool {
    membership.founding_device() == Some(device)
        && membership
            .device(device)
            .is_some_and(|entry| entry.role == Role::Owner)
}

fn is_pairing_owner(
    membership: &Membership,
    configuration: &Configuration,
    device: DeviceId,
) -> bool {
    let Some(id) =
        owner(membership, device).and_then(|enrollment| voter_id(membership, enrollment))
    else {
        return false;
    };
    matches!(configuration.membership().get_joint_config().as_slice(), [voters] if voters.contains(&id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_retried_invite_keeps_the_enrollment_and_binds_the_snapshot_to_both_devices() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.sqlite");
        let identity_dir = temp.path().join("identity");
        let owner_contact = PeerContact {
            certificate: vec![1],
            address_hint: None,
        };
        let joiner_contact = PeerContact {
            certificate: vec![2],
            address_hint: Some("127.0.0.1:37419".parse().unwrap()),
        };
        let owner = owner_contact.device_id();
        let joiner = joiner_contact.device_id();
        let different_device = [3; 32];
        initialize_local_space(
            source.clone(),
            identity_dir.clone(),
            "space".into(),
            "account".into(),
            owner_contact.clone(),
        )
        .await
        .unwrap();
        let mut source = MembershipLog::open(source, "space").unwrap();
        let first = source
            .approve_pairing(
                identity_dir.clone(),
                "account",
                owner,
                joiner_contact.clone(),
                Role::Member,
            )
            .await
            .unwrap()
            .unwrap();
        let retry = source
            .approve_pairing(
                identity_dir.clone(),
                "account",
                owner,
                joiner_contact.clone(),
                Role::Member,
            )
            .await
            .unwrap()
            .unwrap();
        let incoming = serde_json::to_vec(&retry).unwrap();
        let enrolled = |history: &PairingHistory| {
            history
                .0
                .contents
                .state
                .membership
                .as_ref()
                .unwrap()
                .device(joiner)
                .unwrap()
        };
        assert_eq!(enrolled(&first), enrolled(&retry));

        let mut replica = MembershipLog::open(temp.path().join("replica.sqlite"), "space").unwrap();
        assert!(
            replica
                .receive_pairing_history(Some(first), different_device, joiner, None)
                .await
                .is_err()
        );
        assert!(
            replica
                .receive_pairing_history(
                    Some(serde_json::from_slice(&incoming).unwrap()),
                    owner,
                    different_device,
                    None
                )
                .await
                .is_err()
        );
        assert!(replica.membership().await.unwrap().is_none());
        replica
            .receive_pairing_history(Some(retry), owner, joiner, None)
            .await
            .unwrap();
        let remote = replica.membership().await.unwrap().unwrap();
        let local = source.membership().await.unwrap().unwrap();
        assert_eq!(remote.device(joiner), local.device(joiner));
        assert_eq!(remote.device(joiner).unwrap().role, Role::Member);
        assert_eq!(remote.peer_contact(owner), Some(&owner_contact));
        assert_eq!(remote.peer_contact(joiner), Some(&joiner_contact));
        assert_eq!(remote.peer_contacts().count(), 2);

        let changed_address = PeerContact {
            address_hint: Some("127.0.0.1:37420".parse().unwrap()),
            ..joiner_contact
        };
        let updated = source
            .approve_pairing(
                identity_dir.clone(),
                "account",
                owner,
                changed_address.clone(),
                Role::Member,
            )
            .await
            .unwrap()
            .unwrap();
        replica
            .receive_pairing_history(Some(updated), owner, joiner, None)
            .await
            .unwrap();
        let remote = replica.membership().await.unwrap().unwrap();
        assert_eq!(remote.device(joiner), local.device(joiner));
        assert_eq!(remote.peer_contact(joiner), Some(&changed_address));
        assert!(
            replica
                .receive_pairing_history(None, owner, joiner, None)
                .await
                .is_err()
        );

        let third_contact = PeerContact {
            certificate: vec![3],
            address_hint: None,
        };
        let third = third_contact.device_id();
        let history = source
            .approve_pairing(identity_dir, "account", owner, third_contact, Role::Owner)
            .await
            .unwrap()
            .unwrap();
        let mut third_replica =
            MembershipLog::open(temp.path().join("third.sqlite"), "space").unwrap();
        third_replica
            .receive_pairing_history(Some(history), owner, third, Some("account"))
            .await
            .unwrap();
        let roster = third_replica.membership().await.unwrap().unwrap();
        assert_eq!(roster.peer_contact(joiner), Some(&changed_address));
        assert_eq!(roster.peer_contacts().count(), 3);
    }
}
