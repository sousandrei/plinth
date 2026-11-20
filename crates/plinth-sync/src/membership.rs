//! Per-space permissions evaluated in the order of a committed log.
//!
//! The caller must authenticate the author and supply the committed log index.
//! This module does not run elections or authenticate network messages.

use std::collections::BTreeMap;

/// SHA-256 fingerprint of the device's authenticated certificate.
pub type DeviceId = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Enrollment {
    pub device: DeviceId,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
    Member,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    pub enrollment: Enrollment,
    pub role: Role,
}

#[derive(Debug, Clone, Copy)]
pub enum Change {
    Enroll { device: DeviceId, role: Role },
    Remove { enrollment: Enrollment },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Enrolled(Enrollment),
    Removed(Enrollment),
    AuthorNotOwner,
    AlreadyEnrolled,
    EnrollmentInactive,
    LastOwner,
    AlreadyApplied,
}

/// One owner account can authorize several owner devices in this space.
#[derive(Debug, Clone)]
pub struct Membership {
    owner_account: String,
    last_applied: u64,
    devices: BTreeMap<DeviceId, Device>,
    removals: BTreeMap<Enrollment, u64>,
}

impl Membership {
    pub fn new(owner_account: String, first_device: DeviceId, committed_index: u64) -> Self {
        let owner = Device {
            enrollment: Enrollment {
                device: first_device,
                generation: committed_index,
            },
            role: Role::Owner,
        };
        Self {
            owner_account,
            last_applied: committed_index,
            devices: BTreeMap::from([(first_device, owner)]),
            removals: BTreeMap::new(),
        }
    }

    pub fn owner_account(&self) -> &str {
        &self.owner_account
    }

    pub fn device(&self, id: DeviceId) -> Option<Device> {
        self.devices.get(&id).copied()
    }

    pub fn removal_index(&self, enrollment: Enrollment) -> Option<u64> {
        self.removals.get(&enrollment).copied()
    }

    pub fn apply(&mut self, index: u64, author: Enrollment, change: Change) -> Outcome {
        if index <= self.last_applied {
            return Outcome::AlreadyApplied;
        }
        self.last_applied = index;
        if self.device(author.device)
            != Some(Device {
                enrollment: author,
                role: Role::Owner,
            })
        {
            return Outcome::AuthorNotOwner;
        }

        match change {
            Change::Enroll { device, role } => {
                if self.devices.contains_key(&device) {
                    return Outcome::AlreadyEnrolled;
                }
                let enrollment = Enrollment {
                    device,
                    generation: index,
                };
                self.devices.insert(device, Device { enrollment, role });
                Outcome::Enrolled(enrollment)
            }
            Change::Remove { enrollment } => {
                let Some(target) = self.device(enrollment.device) else {
                    return Outcome::EnrollmentInactive;
                };
                if target.enrollment != enrollment {
                    return Outcome::EnrollmentInactive;
                }
                if target.role == Role::Owner
                    && !self
                        .devices
                        .values()
                        .any(|device| device.role == Role::Owner && *device != target)
                {
                    return Outcome::LastOwner;
                }

                self.devices.remove(&enrollment.device);
                self.removals.insert(enrollment, index);
                Outcome::Removed(enrollment)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: DeviceId = [1; 32];
    const B: DeviceId = [2; 32];
    const C: DeviceId = [3; 32];

    fn enroll(
        membership: &mut Membership,
        index: u64,
        author: Enrollment,
        device: DeviceId,
        role: Role,
    ) -> Enrollment {
        let outcome = membership.apply(index, author, Change::Enroll { device, role });
        let Outcome::Enrolled(enrollment) = outcome else {
            panic!("expected enrollment, got {outcome:?}");
        };
        enrollment
    }

    #[test]
    fn competing_removals_follow_commit_order() {
        let mut membership = Membership::new("account".into(), A, 1);
        let a = membership.device(A).unwrap().enrollment;
        let b = enroll(&mut membership, 2, a, B, Role::Owner);
        let c = enroll(&mut membership, 3, a, C, Role::Owner);
        let mut reversed = membership.clone();
        let remove_a = Change::Remove { enrollment: a };
        let remove_b = Change::Remove { enrollment: b };

        assert_eq!(membership.apply(4, c, remove_a), Outcome::Removed(a));
        assert_eq!(membership.apply(5, a, remove_b), Outcome::AuthorNotOwner);
        assert_eq!(membership.removal_index(a), Some(4));
        assert_eq!(membership.removal_index(b), None);
        assert!(membership.device(B).is_some());

        assert_eq!(reversed.apply(4, a, remove_b), Outcome::Removed(b));
        assert_eq!(reversed.apply(5, c, remove_a), Outcome::Removed(a));
        assert_eq!(reversed.removal_index(b), Some(4));
        assert_eq!(reversed.removal_index(a), Some(5));
        assert_eq!(reversed.owner_account(), "account");
    }

    #[test]
    fn old_enrollment_cannot_remove_or_manage_a_fresh_enrollment() {
        let mut membership = Membership::new("account".into(), A, 1);
        let a = membership.device(A).unwrap().enrollment;
        let b = enroll(&mut membership, 2, a, B, Role::Owner);
        let remove_b = Change::Remove { enrollment: b };
        assert_eq!(membership.apply(3, a, remove_b), Outcome::Removed(b));
        let new_b = enroll(&mut membership, 4, a, B, Role::Owner);

        assert_eq!(membership.apply(3, a, remove_b), Outcome::AlreadyApplied);
        assert_eq!(
            membership.apply(5, a, remove_b),
            Outcome::EnrollmentInactive
        );
        assert_eq!(membership.apply(6, b, remove_b), Outcome::AuthorNotOwner);
        assert_eq!(membership.device(B).unwrap().enrollment, new_b);
        assert_eq!(membership.removal_index(b), Some(3));
        assert_eq!(membership.removal_index(new_b), None);
    }

    #[test]
    fn ordinary_members_cannot_manage_devices_and_a_space_keeps_an_owner() {
        let mut membership = Membership::new("account".into(), A, 1);
        let a = membership.device(A).unwrap().enrollment;
        let b = enroll(&mut membership, 2, a, B, Role::Member);
        let remove_a = Change::Remove { enrollment: a };

        assert_eq!(membership.apply(3, b, remove_a), Outcome::AuthorNotOwner);
        assert_eq!(membership.apply(4, a, remove_a), Outcome::LastOwner);
        assert_eq!(membership.device(A).unwrap().enrollment, a);
    }
}
