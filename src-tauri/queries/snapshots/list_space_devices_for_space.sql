-- Snapshot: list space grants and trust modes for a space.
SELECT space_id, device_id, trust_mode, paired_at
FROM space_devices
WHERE space_id = ?1
ORDER BY device_id ASC
