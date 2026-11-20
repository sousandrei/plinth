use super::*;

const RECEIVED_WITHDRAWAL: &str = "received_withdrawal";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Withdrawal {
    space_id: String,
    enrollment: Enrollment,
    index: u64,
    space_deleted: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct WithdrawalEvidence {
    reports: Vec<(Enrollment, Withdrawal)>,
    confirmed: Option<Withdrawal>,
}

pub struct SpaceWithdrawal {
    pub retain_contacts: bool,
}

impl MembershipLog {
    pub async fn local_withdrawal(
        &self,
        local: DeviceId,
    ) -> transport::Result<Option<SpaceWithdrawal>> {
        let withdrawal = self
            .access(
                ErrorSubject::StateMachine,
                ErrorVerb::Read,
                move |db, space| {
                    let state = read_state(db, space)?;
                    let Some(membership) = state.membership else {
                        return Ok(None);
                    };
                    let enrolled = membership.device(local).map(|device| device.enrollment);
                    let deletion = enrolled.is_some() && membership.deletion_index().is_some();
                    let confirmed =
                        read_meta::<WithdrawalEvidence>(db, space, RECEIVED_WITHDRAWAL)?
                            .and_then(|evidence| evidence.confirmed)
                            .is_some_and(|notice| Some(notice.enrollment) == enrolled);
                    Ok(
                        (deletion || membership.removed_device(local) || confirmed).then(|| {
                            SpaceWithdrawal {
                                retain_contacts: deletion
                                    && voter(&membership, &state.voters, local).is_some(),
                            }
                        }),
                    )
                },
            )
            .await?;
        Ok(withdrawal)
    }

    pub async fn withdrawal_for(
        &self,
        local: DeviceId,
        enrollment: Enrollment,
    ) -> transport::Result<Option<Withdrawal>> {
        let state = self.pairing_state().await?;
        let Some(membership) = state.membership else {
            return Ok(None);
        };
        if voter(&membership, &state.voters, local).is_none() {
            return Ok(None);
        }
        let deletion = membership.deletion_index().filter(|_| {
            membership
                .device(enrollment.device)
                .is_some_and(|device| device.enrollment == enrollment)
        });
        Ok(deletion
            .or_else(|| membership.removal_index(enrollment))
            .map(|index| Withdrawal {
                space_id: self.space_id.to_string(),
                enrollment,
                index,
                space_deleted: deletion.is_some(),
            }))
    }

    /// The sender must come from the pinned TLS certificate for this space.
    pub async fn record_withdrawal(
        &self,
        local: DeviceId,
        sender: DeviceId,
        notice: Withdrawal,
    ) -> transport::Result<bool> {
        let accepted = self
            .access(
                ErrorSubject::StateMachine,
                ErrorVerb::Write,
                move |db, space| {
                    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    let state = read_state(&tx, space)?;
                    let Some(membership) = state.membership else {
                        return Ok(false);
                    };
                    let Some((author, _)) = voter(&membership, &state.voters, sender) else {
                        return Ok(false);
                    };
                    if notice.space_id != space
                        || notice.enrollment.device != local
                        || local == sender
                        || !membership
                            .device(local)
                            .is_some_and(|device| device.enrollment == notice.enrollment)
                        || notice.index <= notice.enrollment.generation
                    {
                        return Ok(false);
                    }
                    let mut evidence =
                        read_meta::<WithdrawalEvidence>(&tx, space, RECEIVED_WITHDRAWAL)?
                            .unwrap_or_default();
                    if let Some(recorded) = &evidence.confirmed
                        && recorded.enrollment == notice.enrollment
                    {
                        return Ok(recorded == &notice);
                    }
                    if state
                        .last_applied
                        .is_some_and(|applied| applied.index >= notice.index)
                    {
                        return Ok(false);
                    }
                    evidence.reports.retain(|(reporter, claim)| {
                        claim.enrollment == notice.enrollment && reporter.device != sender
                    });
                    evidence.reports.push((author, notice.clone()));
                    let agreeing: BTreeSet<_> = evidence
                        .reports
                        .iter()
                        .filter_map(|(reporter, claim)| {
                            let (active, id) = voter(&membership, &state.voters, reporter.device)?;
                            (active == *reporter && claim == &notice).then_some(id)
                        })
                        .collect();
                    let configurations = state.voters.membership().get_joint_config();
                    let confirmed = !configurations.is_empty()
                        && configurations.iter().all(|voters| {
                            voters.intersection(&agreeing).count() > voters.len() / 2
                        });
                    evidence.confirmed = confirmed.then_some(notice);
                    write_meta(&tx, space, RECEIVED_WITHDRAWAL, &evidence)?;
                    tx.commit()?;
                    Ok(confirmed)
                },
            )
            .await?;
        Ok(accepted)
    }

    pub async fn received_withdrawal(
        &self,
        local: DeviceId,
    ) -> transport::Result<Option<Withdrawal>> {
        Ok(self
            .access(
                ErrorSubject::StateMachine,
                ErrorVerb::Read,
                move |db, space| {
                    let Some(notice) =
                        read_meta::<WithdrawalEvidence>(db, space, RECEIVED_WITHDRAWAL)?
                            .and_then(|evidence| evidence.confirmed)
                    else {
                        return Ok(None);
                    };
                    Ok(read_state(db, space)?.membership.and_then(|membership| {
                        membership
                            .device(local)
                            .filter(|device| device.enrollment == notice.enrollment)
                            .map(|_| notice)
                    }))
                },
            )
            .await?)
    }
}

fn voter(
    membership: &Membership,
    configuration: &Configuration,
    device: DeviceId,
) -> Option<(Enrollment, u64)> {
    let enrollment = replica::owner(membership, device)?;
    let id = replica::voter_id(membership, enrollment)?;
    configuration
        .voter_ids()
        .any(|voter| voter == id)
        .then_some((enrollment, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::membership::Role;

    #[tokio::test]
    async fn withdrawal_is_scoped_to_the_space_and_enrollment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.sqlite");
        let source = MembershipLog::open(&path, "shared").unwrap();
        let target = MembershipLog::open(dir.path().join("target.sqlite"), "shared").unwrap();
        let owner = Enrollment {
            device: [1; 32],
            generation: 1,
        };
        let user = [2; 32];
        let enroll = Change::Enroll {
            device: user,
            role: Role::Member,
        };
        let mut membership = Membership::new("owner".into(), owner.device, owner.generation);
        let Outcome::Enrolled(old) = membership.apply(2, owner, enroll.clone()) else {
            panic!("initial enrollment failed");
        };
        save_membership(&target, membership.clone()).await;
        assert_eq!(
            membership.apply(3, owner, Change::Remove { enrollment: old }),
            Outcome::Removed(old)
        );
        save_membership(&source, membership.clone()).await;
        let removal = source
            .withdrawal_for(owner.device, old)
            .await
            .unwrap()
            .unwrap();
        assert!(!removal.space_deleted);
        let mut wrong_space = removal.clone();
        wrong_space.space_id = "unrelated".into();
        assert!(
            !target
                .record_withdrawal(user, owner.device, wrong_space)
                .await
                .unwrap()
        );
        assert!(
            target
                .record_withdrawal(user, owner.device, removal.clone())
                .await
                .unwrap()
        );

        let Outcome::Enrolled(fresh) = membership.apply(4, owner, enroll) else {
            panic!("fresh enrollment failed");
        };
        save_membership(&target, membership.clone()).await;
        assert!(target.received_withdrawal(user).await.unwrap().is_none());
        assert!(
            !target
                .record_withdrawal(user, owner.device, removal)
                .await
                .unwrap()
        );

        assert_eq!(
            membership.apply(5, owner, Change::DeleteSpace),
            Outcome::SpaceDeleted
        );
        save_membership(&source, membership).await;
        let deletion = source
            .withdrawal_for(owner.device, fresh)
            .await
            .unwrap()
            .unwrap();
        assert!(deletion.space_deleted);
        assert!(
            target
                .record_withdrawal(user, owner.device, deletion.clone())
                .await
                .unwrap()
        );
        assert_eq!(
            target.received_withdrawal(user).await.unwrap(),
            Some(deletion)
        );
    }

    async fn save_membership(log: &MembershipLog, membership: Membership) {
        let state = AppliedState {
            membership: Some(membership),
            voters: StoredMembership::new(
                None,
                openraft::Membership::new(vec![BTreeSet::from([FIRST_VOTER])], ()),
            ),
            ..Default::default()
        };
        log.access(
            ErrorSubject::StateMachine,
            ErrorVerb::Write,
            move |db, space| write_meta(db, space, "state", &state),
        )
        .await
        .unwrap();
    }
}
