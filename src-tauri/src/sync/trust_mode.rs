//! Trust modes for `space_devices` grants.
//!
//! Step 30.4 replaces the legacy `sync_enabled` boolean with an explicit
//! state machine. Each grant lives in exactly one of three states; the
//! fourth ("revoked") is the absence of a row entirely, which removes
//! the cert from the TLS trust set automatically.
//!
//! Lifecycle:
//!
//! ```text
//!   active  ──(local user revokes)──▶  revoking  ──(acked everywhere + row deleted)──▶  (gone)
//!      │
//!      └─(paired without sync privileges)──▶  revocation_only
//! ```
//!
//! `revoking` is a *local* fence outbound — we stop shipping new batches
//! to the peer but the change_log row carrying the transition is the
//! revocation record itself, so it must still propagate.

use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::sqlite::{SqliteArgumentsBuffer, SqliteValueRef};
use sqlx::{Decode, Encode, Sqlite, Type};

use crate::error::AppError;

/// State of a `space_devices` grant.
///
/// Note: a fourth state, `revoked`, exists implicitly as the *absence* of
/// a row. Code that needs to detect revocation should compare against
/// the grant set, not this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustMode {
    /// Normal ping + bidirectional sync.
    Active,
    /// Local fence outbound; revocation is propagating through change_log.
    /// The peer may still ack incoming batches but won't receive any new
    /// ones from us.
    Revoking,
    /// Peer may not participate in normal sync. Used for downgraded
    /// devices that should only handle revocation traffic.
    RevocationOnly,
}

impl TrustMode {
    /// Canonical lowercase string used both in SQL and on the wire.
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustMode::Active => "active",
            TrustMode::Revoking => "revoking",
            TrustMode::RevocationOnly => "revocation_only",
        }
    }

    /// Inverse of `as_str`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(TrustMode::Active),
            "revoking" => Some(TrustMode::Revoking),
            "revocation_only" => Some(TrustMode::RevocationOnly),
            _ => None,
        }
    }

    /// True if a grant in this state should ship outbound batches.
    /// `revoking` is a local fence — we stop pushing. `revocation_only`
    /// is restricted to revocation traffic only.
    #[allow(dead_code)] // used by Step 30.4b session.rs
    pub fn ships_outbound(&self) -> bool {
        matches!(self, TrustMode::Active)
    }

    /// True if a grant in this state should accept inbound batches.
    /// `revoking` peers still need to receive the revocation record
    /// itself, so they accept inbound — but no new data flows until
    /// they finish acking. `revocation_only` is read-only by definition.
    #[allow(dead_code)] // used by Step 30.4b session.rs
    pub fn accepts_inbound(&self) -> bool {
        matches!(self, TrustMode::Active | TrustMode::Revoking)
    }
}

impl Type<Sqlite> for TrustMode {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <&str as Type<Sqlite>>::type_info()
    }
}

impl<'r> Decode<'r, Sqlite> for TrustMode {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as Decode<Sqlite>>::decode(value)?;
        Self::parse(s).ok_or_else(|| format!("unknown trust_mode: {s:?}").into())
    }
}

impl<'q> Encode<'q, Sqlite> for TrustMode {
    fn encode_by_ref(
        &self,
        buf: &mut SqliteArgumentsBuffer,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        <String as Encode<'q, Sqlite>>::encode_by_ref(&self.as_str().to_string(), buf)
    }
}

impl std::fmt::Display for TrustMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TrustMode {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| AppError::InvalidInput(format!("trust_mode: {s}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_str() {
        for m in [
            TrustMode::Active,
            TrustMode::Revoking,
            TrustMode::RevocationOnly,
        ] {
            assert_eq!(TrustMode::parse(m.as_str()), Some(m));
        }
    }

    #[test]
    fn unknown_string_is_none() {
        assert_eq!(TrustMode::parse("revoked"), None);
        assert_eq!(TrustMode::parse(""), None);
        assert_eq!(TrustMode::parse("ACTIVE"), None);
    }

    #[test]
    fn serde_json_round_trip() {
        let cases = [
            (TrustMode::Active, "\"active\""),
            (TrustMode::Revoking, "\"revoking\""),
            (TrustMode::RevocationOnly, "\"revocation_only\""),
        ];
        for (m, expected) in cases {
            assert_eq!(serde_json::to_string(&m).unwrap(), expected);
            assert_eq!(serde_json::from_str::<TrustMode>(expected).unwrap(), m);
        }
    }

    #[test]
    fn gates_match_plan() {
        // Step 30.4: only `active` ships outbound. `revoking` is a
        // local fence; `revocation_only` is restricted entirely.
        // Inbound accepts both `active` and `revoking` — a revoking
        // peer still needs to receive the revocation record.
        assert!(TrustMode::Active.ships_outbound());
        assert!(TrustMode::Active.accepts_inbound());
        assert!(!TrustMode::Revoking.ships_outbound());
        assert!(TrustMode::Revoking.accepts_inbound());
        assert!(!TrustMode::RevocationOnly.ships_outbound());
        assert!(!TrustMode::RevocationOnly.accepts_inbound());
    }

    #[test]
    fn from_str_rejects_unknown() {
        assert!("nonsense".parse::<TrustMode>().is_err());
    }
}
