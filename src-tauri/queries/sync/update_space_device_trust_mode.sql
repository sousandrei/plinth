-- Set the trust_mode of an existing (space, device) grant. Used by the
-- `set_space_device_trust_mode` Tauri command to transition a grant
-- between Step 30.4 states ('active' ↔ 'revoking' ↔ 'revocation_only').
-- The change_log AU trigger captures the transition so all peers see
-- it on their next sync round.
UPDATE space_devices
SET trust_mode = ?3
WHERE space_id = ?1 AND device_id = ?2
