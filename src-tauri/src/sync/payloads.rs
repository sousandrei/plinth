use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// One payload struct per synced table. Field names and types must match
// exactly what the SQLite triggers produce via `json_object(...)` in
// `migrations/0001_initial_schema.sql`. INTEGER columns map to `i64`,
// TEXT to `String`, and nullable columns to `Option<_>`.
//
// LWW timestamps live on `wire::ChangeRow.changed_at`, not inside any
// payload — every table is treated uniformly by the applier, including
// tables without their own `updated_at` column (transactions,
// space_settings, account_summaries).
//
// `transactions` payload deliberately has no `space_id`: the trigger
// derives `change_log.space_id` from a JOIN, but the payload itself
// only carries the columns of the row. The applier reads the space
// from `ChangeRow.space_id`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacePayload {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Embedded inside `SpaceMemberPayload.user` so a newly added member's
/// `users` row materializes on every peer that shares the space. See
/// `data/PLAN.md §4` and the `change_log_space_members_*` triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSnapshot {
    pub id: String,
    pub name: String,
    pub pin_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceMemberPayload {
    pub space_id: String,
    pub user_id: String,
    pub role: String,
    pub joined_at: String,
    pub user: Option<UserSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPayload {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub account_type: String,
    pub account_source: String,
    pub color: String,
    pub space_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryPayload {
    pub id: String,
    pub name: String,
    pub color: String,
    pub space_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionPayload {
    pub id: String,
    pub booking_date: String,
    pub value_date: String,
    pub reference: String,
    pub text: String,
    pub currency: String,
    pub amount: i64,
    pub balance: i64,
    pub approved: i64,
    pub note: String,
    pub category: Option<String>,
    pub account_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummaryPayload {
    pub month: String,
    pub account_id: String,
    pub balance: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSettingPayload {
    pub space_id: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedDevicePayload {
    pub id: String,
    pub space_id: String,
    pub device_id: String,
    pub display_name: String,
    pub cert_pem: String,
    pub sync_enabled: i64,
    pub paired_at: String,
}

/// One row of the `model_versions` registry — the mesh-wide record of
/// which trained-model versions exist, authored by whom, and their
/// MD5s for transfer integrity. The on-disk weights + card files are
/// transferred separately via `Frame::ModelData`; this payload only
/// describes the row. See `data/PLAN.md §Phase 21`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersionPayload {
    pub space_id: String,
    pub version: u32,
    pub weights_md5: String,
    pub card_md5: String,
    pub trained_at: String,
}

/// Tagged union of all synced table payloads. The discriminant is the
/// SQLite table name from `change_log.table_name` — `from_json` matches
/// on it and `as_table_name` is the inverse, useful for diagnostics.
///
/// Postcard encodes the variant as a one-byte tag; on the wire this is
/// strictly smaller than the original JSON-in-JSON form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TablePayload {
    Space(SpacePayload),
    SpaceMember(SpaceMemberPayload),
    Account(AccountPayload),
    Category(CategoryPayload),
    Transaction(TransactionPayload),
    AccountSummary(AccountSummaryPayload),
    SpaceSetting(SpaceSettingPayload),
    TrustedDevice(TrustedDevicePayload),
    ModelVersion(ModelVersionPayload),
}

impl TablePayload {
    /// Round-trip helper for `change_log.table_name` strings. Returns
    /// the canonical SQLite table name corresponding to this variant.
    pub fn as_table_name(&self) -> &'static str {
        match self {
            Self::Space(_) => "spaces",
            Self::SpaceMember(_) => "space_members",
            Self::Account(_) => "accounts",
            Self::Category(_) => "categories",
            Self::Transaction(_) => "transactions",
            Self::AccountSummary(_) => "account_summaries",
            Self::SpaceSetting(_) => "space_settings",
            Self::TrustedDevice(_) => "trusted_devices",
            Self::ModelVersion(_) => "model_versions",
        }
    }
}

/// Parse a `change_log.payload` JSON string into a typed `TablePayload`,
/// dispatching on `table_name`. Returns `Err(PayloadError::UnknownTable)`
/// for tables that this build doesn't know how to apply — those should
/// never reach a synced peer unless the protocol versions disagree.
pub fn from_json(table_name: &str, json: &str) -> Result<TablePayload, PayloadError> {
    macro_rules! decode {
        ($variant:ident, $ty:ty) => {
            serde_json::from_str::<$ty>(json)
                .map(TablePayload::$variant)
                .map_err(|e| PayloadError::Decode {
                    table: table_name.to_string(),
                    source: e,
                })
        };
    }
    match table_name {
        "spaces" => decode!(Space, SpacePayload),
        "space_members" => decode!(SpaceMember, SpaceMemberPayload),
        "accounts" => decode!(Account, AccountPayload),
        "categories" => decode!(Category, CategoryPayload),
        "transactions" => decode!(Transaction, TransactionPayload),
        "account_summaries" => decode!(AccountSummary, AccountSummaryPayload),
        "space_settings" => decode!(SpaceSetting, SpaceSettingPayload),
        "trusted_devices" => decode!(TrustedDevice, TrustedDevicePayload),
        "model_versions" => decode!(ModelVersion, ModelVersionPayload),
        other => Err(PayloadError::UnknownTable(other.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("unknown synced table: {0}")]
    UnknownTable(String),

    #[error("decode payload for table {table}: {source}")]
    Decode {
        table: String,
        #[source]
        source: serde_json::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_transaction_payload_with_nullable_category() {
        let json = r#"{
            "id": "tx-1",
            "booking_date": "2024-01-01",
            "value_date": "2024-01-01",
            "reference": "",
            "text": "lunch",
            "currency": "SEK",
            "amount": -12500,
            "balance": 50000,
            "approved": 0,
            "note": "",
            "category": null,
            "account_id": "acc-1"
        }"#;
        let p = from_json("transactions", json).unwrap();
        let TablePayload::Transaction(t) = p else {
            panic!("wrong variant");
        };
        assert_eq!(t.amount, -12500);
        assert!(t.category.is_none());
    }

    #[test]
    fn decodes_space_member_with_embedded_user() {
        let json = r#"{
            "space_id": "sp-1",
            "user_id": "u-1",
            "role": "owner",
            "joined_at": "2024-01-01T00:00:00Z",
            "user": {
                "id": "u-1",
                "name": "Alice",
                "pin_hash": "argon2:...",
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z"
            }
        }"#;
        let p = from_json("space_members", json).unwrap();
        let TablePayload::SpaceMember(m) = p else {
            panic!("wrong variant");
        };
        let user = m.user.unwrap();
        assert_eq!(user.id, "u-1");
        assert_eq!(user.name, "Alice");
    }

    #[test]
    fn unknown_table_is_an_error() {
        let err = from_json("widgets", "{}").unwrap_err();
        assert!(matches!(err, PayloadError::UnknownTable(_)));
    }

    #[test]
    fn as_table_name_round_trips() {
        let p = TablePayload::Space(SpacePayload {
            id: "x".into(),
            name: "n".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
        });
        assert_eq!(p.as_table_name(), "spaces");
    }
}
