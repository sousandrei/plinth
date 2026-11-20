use super::*;
use crate::membership::Role;

/// A permission snapshot sent over a confirmed pairing connection while the
/// originating device is the sole voter.
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
        if !state.voters.voter_ids().eq([FIRST_VOTER])
            || membership.owner_account() != owner_account
            || !is_founder(&membership, local_device)
        {
            return Err("pair devices from the computer that created this space".into());
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
        joining_device: DeviceId,
        role: Role,
    ) -> transport::Result<Option<PairingHistory>> {
        if !self.owner_can_pair(owner_account, local_device).await? {
            return Ok(None);
        }
        if local_device == joining_device {
            return Err("cannot pair a device with itself".into());
        }
        let membership = self
            .membership()
            .await?
            .ok_or("missing permission history")?;
        let author = membership
            .device(local_device)
            .ok_or("missing owner device")?
            .enrollment;
        if let Some(device) = membership.device(joining_device) {
            if device.role != role {
                return Err("this device already has a different role in this space".into());
            }
        } else {
            let network =
                rpc::PermissionNetwork::new(identity_dir, self.space_id.clone(), |_| None);
            let node = Raft::new(
                FIRST_VOTER,
                Arc::new(Config::default()),
                network,
                self.clone(),
                self.clone(),
            )
            .await?;
            let approved = tokio::time::timeout(LOCAL_WRITE_TIMEOUT, async {
                node.wait(Some(LOCAL_WRITE_TIMEOUT))
                    .current_leader(FIRST_VOTER, "pairing owner")
                    .await?;
                let response = node
                    .client_write(MembershipRequest::Change {
                        author,
                        change: Change::Enroll {
                            device: joining_device,
                            role,
                        },
                    })
                    .await?;
                if !matches!(response.data, Outcome::Enrolled(_)) {
                    return Err(format!("could not enroll device: {:?}", response.data).into());
                }
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
            })
            .await;
            let stopped = node.shutdown().await;
            approved??;
            stopped?;
        }
        let snapshot = self.build_snapshot().await?;
        Ok(Some(PairingHistory(StoredSnapshot {
            meta: snapshot.meta,
            contents: serde_json::from_slice(snapshot.snapshot.get_ref())?,
        })))
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
            || !snapshot.meta.last_membership.voter_ids().eq([FIRST_VOTER])
            || !is_founder(incoming, host)
            || !incoming
                .device(local_device)
                .is_some_and(|device| device.role == expected_role)
            || current.as_ref().is_some_and(|existing| {
                existing.owner_account() != incoming.owner_account()
                    || existing.founding_device() != Some(host)
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
    async fn pairing_state(&self) -> RaftResult<AppliedState> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_retried_invite_keeps_the_enrollment_and_binds_the_snapshot_to_both_devices() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.sqlite");
        let identity_dir = temp.path().join("identity");
        let owner = [1; 32];
        let joiner = [2; 32];
        let different_device = [3; 32];
        initialize_local_space(
            source.clone(),
            identity_dir.clone(),
            "space".into(),
            "account".into(),
            owner,
        )
        .await
        .unwrap();
        let mut source = MembershipLog::open(source, "space").unwrap();
        let first = source
            .approve_pairing(identity_dir.clone(), "account", owner, joiner, Role::Member)
            .await
            .unwrap()
            .unwrap();
        let retry = source
            .approve_pairing(identity_dir, "account", owner, joiner, Role::Member)
            .await
            .unwrap()
            .unwrap();
        let incoming = serde_json::to_vec(&retry).unwrap();
        assert_eq!(serde_json::to_vec(&first).unwrap(), incoming);

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
        assert!(
            replica
                .receive_pairing_history(None, owner, joiner, None)
                .await
                .is_err()
        );
    }
}
