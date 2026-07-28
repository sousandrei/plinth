-- Upsert a space_device grant during remote apply. The stub `devices`
-- row is created in Rust (upsert_space_device) before this query runs
-- to satisfy the FK constraint. Replaces apply/upsert_trusted_device.sql.
-- No random id — keyed by (space_id, device_id).
INSERT INTO space_devices (space_id, device_id, sync_enabled, paired_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT (space_id, device_id) DO UPDATE
SET sync_enabled = excluded.sync_enabled
