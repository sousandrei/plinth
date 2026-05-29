use crate::sync::payloads::TransactionPayload;

/// Tolerance window for hot-field conflict detection. If the local and
/// remote changes to the same transaction field were made within this
/// many seconds of each other on different devices, the user is asked
/// to resolve — outside this window, LWW silently wins. See
/// `data/PLAN.md §7.1`.
pub const HOT_FIELD_TOLERANCE_SECS: i64 = 60;

/// The transaction fields where silent LWW is too lossy. Any other
/// field difference is resolved by `changed_at` without surfacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotField {
    Category,
    Note,
}

impl HotField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Note => "note",
        }
    }
}

/// One field-level conflict that the applier must record in
/// `sync_conflicts` instead of overwriting silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotFieldConflict {
    pub field: HotField,
    pub local_value: Option<String>,
    pub remote_value: Option<String>,
    pub local_changed_at: String,
    pub remote_changed_at: String,
}

/// Inputs describing the local side of a candidate conflict — the
/// current row state and the `changed_at` of the most recent local
/// `change_log` entry for that row authored by a *different* device.
/// Callers fetch this from SQLite; the detector itself stays pure so
/// the timing rules are unit-testable in isolation.
///
/// `last_local_changed_at` is `None` when the local change_log has no
/// prior entry from a different device for this row — in that case no
/// hot-field conflict is possible and `detect` returns an empty Vec.
#[derive(Debug, Clone)]
pub struct LocalSnapshot<'a> {
    pub category: Option<&'a str>,
    pub note: &'a str,
    pub last_local_changed_at: Option<&'a str>,
    pub last_local_device_id: Option<&'a str>,
}

/// Returns one `HotFieldConflict` per hot field whose value differs
/// between local and incoming, where the two changes were authored on
/// different devices within `HOT_FIELD_TOLERANCE_SECS` of each other.
///
/// Pure function: all timing inputs are passed as ISO 8601 strings
/// matching the format used in `change_log.changed_at`.
pub fn detect_hot_field_conflict(
    local: &LocalSnapshot<'_>,
    remote_device_id: &str,
    remote_changed_at: &str,
    remote: &TransactionPayload,
) -> Vec<HotFieldConflict> {
    let (Some(local_changed_at), Some(local_device_id)) =
        (local.last_local_changed_at, local.last_local_device_id)
    else {
        return Vec::new();
    };
    if local_device_id == remote_device_id {
        return Vec::new();
    }
    if !within_tolerance(
        local_changed_at,
        remote_changed_at,
        HOT_FIELD_TOLERANCE_SECS,
    ) {
        return Vec::new();
    }

    let mut conflicts = Vec::new();

    let local_cat = local.category.map(str::to_string);
    let remote_cat = remote.category.clone();
    if local_cat != remote_cat {
        conflicts.push(HotFieldConflict {
            field: HotField::Category,
            local_value: local_cat,
            remote_value: remote_cat,
            local_changed_at: local_changed_at.to_string(),
            remote_changed_at: remote_changed_at.to_string(),
        });
    }

    if local.note != remote.note {
        conflicts.push(HotFieldConflict {
            field: HotField::Note,
            local_value: Some(local.note.to_string()),
            remote_value: Some(remote.note.clone()),
            local_changed_at: local_changed_at.to_string(),
            remote_changed_at: remote_changed_at.to_string(),
        });
    }

    conflicts
}

/// Returns true if `a` and `b` (ISO 8601 `YYYY-MM-DDTHH:MM:SSZ`) are
/// within `tolerance_secs` seconds of each other. Returns false if
/// either timestamp fails to parse — a malformed timestamp shouldn't
/// be treated as conflicting, the LWW fallback will sort it out.
fn within_tolerance(a: &str, b: &str, tolerance_secs: i64) -> bool {
    let (Some(a_ts), Some(b_ts)) = (parse_iso(a), parse_iso(b)) else {
        return false;
    };
    (a_ts - b_ts).abs() <= tolerance_secs
}

/// Bare-bones ISO 8601 parser for the strict `YYYY-MM-DDTHH:MM:SSZ`
/// shape emitted by `strftime('%Y-%m-%dT%H:%M:%SZ', 'now')` in every
/// schema default and trigger. Pulling in `chrono` for this would
/// double the dependency footprint for a single arithmetic op.
fn parse_iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let year: i64 = std::str::from_utf8(&b[0..4]).ok()?.parse().ok()?;
    let month: i64 = std::str::from_utf8(&b[5..7]).ok()?.parse().ok()?;
    let day: i64 = std::str::from_utf8(&b[8..10]).ok()?.parse().ok()?;
    let hour: i64 = std::str::from_utf8(&b[11..13]).ok()?.parse().ok()?;
    let minute: i64 = std::str::from_utf8(&b[14..16]).ok()?.parse().ok()?;
    let second: i64 = std::str::from_utf8(&b[17..19]).ok()?.parse().ok()?;
    // Days-from-civil algorithm (Howard Hinnant). Works for any
    // proleptic Gregorian date; cheap and dependency-free.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(category: Option<&str>, note: &str) -> TransactionPayload {
        TransactionPayload {
            id: "tx-1".into(),
            booking_date: "2024-01-01".into(),
            value_date: "2024-01-01".into(),
            reference: String::new(),
            text: "lunch".into(),
            currency: "SEK".into(),
            amount: -12500,
            balance: 0,
            approved: 0,
            note: note.into(),
            category: category.map(str::to_string),
            account_id: "acc-1".into(),
        }
    }

    #[test]
    fn iso_parser_handles_strftime_format() {
        let a = parse_iso("2024-01-01T00:00:00Z").unwrap();
        let b = parse_iso("2024-01-01T00:01:00Z").unwrap();
        assert_eq!(b - a, 60);
    }

    #[test]
    fn iso_parser_rejects_malformed() {
        assert!(parse_iso("not-a-date").is_none());
        assert!(parse_iso("2024-01-01 00:00:00").is_none());
    }

    #[test]
    fn no_local_history_means_no_conflict() {
        let local = LocalSnapshot {
            category: Some("food"),
            note: "lunch",
            last_local_changed_at: None,
            last_local_device_id: None,
        };
        let r = tx(Some("travel"), "taxi");
        let out = detect_hot_field_conflict(&local, "dev-B", "2024-01-01T00:00:00Z", &r);
        assert!(out.is_empty());
    }

    #[test]
    fn same_device_never_conflicts() {
        let local = LocalSnapshot {
            category: Some("food"),
            note: "lunch",
            last_local_changed_at: Some("2024-01-01T00:00:00Z"),
            last_local_device_id: Some("dev-A"),
        };
        let r = tx(Some("travel"), "taxi");
        let out = detect_hot_field_conflict(&local, "dev-A", "2024-01-01T00:00:30Z", &r);
        assert!(out.is_empty());
    }

    #[test]
    fn outside_tolerance_window_means_no_conflict() {
        let local = LocalSnapshot {
            category: Some("food"),
            note: "lunch",
            last_local_changed_at: Some("2024-01-01T00:00:00Z"),
            last_local_device_id: Some("dev-A"),
        };
        let r = tx(Some("travel"), "taxi");
        // 61s gap — LWW silently wins
        let out = detect_hot_field_conflict(&local, "dev-B", "2024-01-01T00:01:01Z", &r);
        assert!(out.is_empty());
    }

    #[test]
    fn within_window_surfaces_both_hot_fields() {
        let local = LocalSnapshot {
            category: Some("food"),
            note: "lunch",
            last_local_changed_at: Some("2024-01-01T00:00:00Z"),
            last_local_device_id: Some("dev-A"),
        };
        let r = tx(Some("travel"), "taxi");
        let out = detect_hot_field_conflict(&local, "dev-B", "2024-01-01T00:00:30Z", &r);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].field, HotField::Category);
        assert_eq!(out[0].local_value.as_deref(), Some("food"));
        assert_eq!(out[0].remote_value.as_deref(), Some("travel"));
        assert_eq!(out[1].field, HotField::Note);
    }

    #[test]
    fn equal_values_never_surface() {
        let local = LocalSnapshot {
            category: Some("food"),
            note: "lunch",
            last_local_changed_at: Some("2024-01-01T00:00:00Z"),
            last_local_device_id: Some("dev-A"),
        };
        let r = tx(Some("food"), "lunch");
        let out = detect_hot_field_conflict(&local, "dev-B", "2024-01-01T00:00:30Z", &r);
        assert!(out.is_empty());
    }

    #[test]
    fn null_to_value_category_change_surfaces() {
        let local = LocalSnapshot {
            category: None,
            note: "lunch",
            last_local_changed_at: Some("2024-01-01T00:00:00Z"),
            last_local_device_id: Some("dev-A"),
        };
        let r = tx(Some("food"), "lunch");
        let out = detect_hot_field_conflict(&local, "dev-B", "2024-01-01T00:00:30Z", &r);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].field, HotField::Category);
        assert_eq!(out[0].local_value, None);
        assert_eq!(out[0].remote_value.as_deref(), Some("food"));
    }
}
