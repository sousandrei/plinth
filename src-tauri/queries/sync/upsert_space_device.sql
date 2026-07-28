-- Grant a device access to a space. One row per (space_id, device_id).
-- `trust_mode` is one of 'active', 'revoking', 'revocation_only'.
INSERT INTO space_devices (space_id, device_id, trust_mode, paired_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT (space_id, device_id) DO UPDATE
SET trust_mode = excluded.trust_mode
