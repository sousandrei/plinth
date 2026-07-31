INSERT INTO space_devices (space_id, device_id, trust_mode, paired_at)
SELECT ?1, ?2, ?3, ?4
WHERE NOT EXISTS (
    SELECT 1 FROM durable_revocations
    WHERE space_id = ?1
      AND target_device_id = ?2
      AND winning_revision >= ?5
)
ON CONFLICT (space_id, device_id) DO UPDATE
SET trust_mode = excluded.trust_mode
WHERE NOT EXISTS (
    SELECT 1 FROM durable_revocations
    WHERE space_id = ?1
      AND target_device_id = ?2
      AND winning_revision >= ?5
)
