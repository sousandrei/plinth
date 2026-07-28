-- Grant a device access to a space. One row per (space_id, device_id).
INSERT INTO space_devices (space_id, device_id, sync_enabled, paired_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT (space_id, device_id) DO UPDATE
SET sync_enabled = excluded.sync_enabled
