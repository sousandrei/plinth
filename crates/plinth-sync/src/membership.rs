//! Per-space permissions evaluated in the order of a committed log.
//!
//! The caller must authenticate the author and supply the committed log index.
//! This module does not run elections or authenticate network messages.

use std::{collections::BTreeMap, net::SocketAddr};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// SHA-256 fingerprint of the device's authenticated certificate.
pub type DeviceId = [u8; 32];

pub fn certificate_fingerprint(certificate: &[u8]) -> DeviceId {
    Sha256::digest(certificate).into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerContact {
    pub certificate: Vec<u8>,
    pub address_hint: Option<SocketAddr>,
}

impl PeerContact {
    pub fn device_id(&self) -> DeviceId {
        certificate_fingerprint(&self.certificate)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Enrollment {
    pub device: DeviceId,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Owner,
    Member,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    pub enrollment: Enrollment,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Change {
    Enroll { device: DeviceId, role: Role },
    EnrollPeer { contact: PeerContact, role: Role },
    Remove { enrollment: Enrollment },
    DeleteSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    NoChange,
    Initialized,
    AlreadyInitialized,
    Uninitialized,
    Enrolled(Enrollment),
    ContactUpdated(Enrollment),
    Removed(Enrollment),
    SpaceDeleted,
    SpaceAlreadyDeleted,
    AuthorNotOwner,
    AlreadyEnrolled,
    EnrollmentInactive,
    LastOwner,
    AlreadyApplied,
}

/// One owner account can authorize several owner devices in this space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    owner_account: String,
    last_applied: u64,
    #[serde(default)]
    deletion_index: Option<u64>,
    #[serde(with = "entries")]
    devices: BTreeMap<DeviceId, Device>,
    #[serde(with = "entries")]
    removals: BTreeMap<Enrollment, u64>,
    #[serde(default, with = "entries")]
    contacts: BTreeMap<DeviceId, PeerContact>,
}

mod entries {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<K: Serialize, V: Serialize, S: Serializer>(
        map: &BTreeMap<K, V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(map)
    }

    pub fn deserialize<'de, K, V, D>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Ok(Vec::<(K, V)>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
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
            deletion_index: None,
            devices: BTreeMap::from([(first_device, owner)]),
            removals: BTreeMap::new(),
            contacts: BTreeMap::new(),
        }
    }

    pub fn owner_account(&self) -> &str {
        &self.owner_account
    }

    pub fn deletion_index(&self) -> Option<u64> {
        self.deletion_index
    }

    pub fn permits_sync(&self, device: DeviceId) -> bool {
        self.deletion_index.is_none() && self.device(device).is_some()
    }

    pub(crate) fn founding_device(&self) -> Option<DeviceId> {
        self.founding_enrollment()
            .map(|enrollment| enrollment.device)
    }

    pub(crate) fn founding_enrollment(&self) -> Option<Enrollment> {
        self.devices
            .values()
            .map(|device| device.enrollment)
            .chain(self.removals.keys().copied())
            .min_by_key(|enrollment| enrollment.generation)
    }

    pub(crate) fn enrolled_owners(&self) -> impl Iterator<Item = Enrollment> + '_ {
        self.devices
            .values()
            .filter(|device| device.role == Role::Owner)
            .map(|device| device.enrollment)
    }

    pub fn device(&self, id: DeviceId) -> Option<Device> {
        self.devices.get(&id).copied()
    }

    pub fn peer_contacts(&self) -> impl Iterator<Item = (Device, &PeerContact)> {
        self.contacts.iter().filter_map(|(id, contact)| {
            self.device(*id)
                .filter(|_| contact.device_id() == *id)
                .map(|device| (device, contact))
        })
    }

    pub fn peer_contact(&self, id: DeviceId) -> Option<&PeerContact> {
        self.device(id)?;
        self.contacts
            .get(&id)
            .filter(|contact| contact.device_id() == id)
    }

    pub(crate) fn record_contact(&mut self, id: DeviceId, contact: PeerContact) {
        if self.devices.contains_key(&id) && contact.device_id() == id {
            self.contacts.insert(id, contact);
        }
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
        if self.deletion_index.is_some() {
            return Outcome::SpaceAlreadyDeleted;
        }

        match change {
            Change::Enroll { device, role } => self.enroll(index, device, role, None),
            Change::EnrollPeer { contact, role } => {
                self.enroll(index, contact.device_id(), role, Some(contact))
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
                self.contacts.remove(&enrollment.device);
                self.removals.insert(enrollment, index);
                Outcome::Removed(enrollment)
            }
            Change::DeleteSpace => {
                self.deletion_index = Some(index);
                Outcome::SpaceDeleted
            }
        }
    }

    fn enroll(
        &mut self,
        index: u64,
        device: DeviceId,
        role: Role,
        contact: Option<PeerContact>,
    ) -> Outcome {
        if let Some(existing) = self.device(device) {
            if let Some(contact) = contact
                && existing.role == role
                && self.contacts.get(&device) != Some(&contact)
            {
                self.record_contact(device, contact);
                return Outcome::ContactUpdated(existing.enrollment);
            }
            return Outcome::AlreadyEnrolled;
        }
        let enrollment = Enrollment {
            device,
            generation: index,
        };
        self.devices.insert(device, Device { enrollment, role });
        if let Some(contact) = contact {
            self.record_contact(device, contact);
        }
        Outcome::Enrolled(enrollment)
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

        assert_eq!(
            membership.apply(4, c, remove_a.clone()),
            Outcome::Removed(a)
        );
        assert_eq!(
            membership.apply(5, a, remove_b.clone()),
            Outcome::AuthorNotOwner
        );
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
        assert_eq!(
            membership.apply(3, a, remove_b.clone()),
            Outcome::Removed(b)
        );
        let new_b = enroll(&mut membership, 4, a, B, Role::Owner);

        assert_eq!(
            membership.apply(3, a, remove_b.clone()),
            Outcome::AlreadyApplied
        );
        assert_eq!(
            membership.apply(5, a, remove_b.clone()),
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

        assert_eq!(
            membership.apply(3, b, remove_a.clone()),
            Outcome::AuthorNotOwner
        );
        assert_eq!(membership.apply(4, a, remove_a), Outcome::LastOwner);
        assert_eq!(membership.device(A).unwrap().enrollment, a);
    }
}
