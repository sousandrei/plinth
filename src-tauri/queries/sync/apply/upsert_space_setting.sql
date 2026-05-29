-- Byte-identical today to `queries/spaces/upsert_space_setting.sql`,
-- intentionally duplicated. See `apply/upsert_account_summary.sql` for
-- the rationale.
INSERT INTO space_settings (space_id, key, value)
VALUES (?1, ?2, ?3)
ON CONFLICT(space_id, key) DO UPDATE SET
    value = excluded.value;
