-- Delete change_log rows that have been acknowledged by every
-- required device. A required device is any trusted_devices entry
-- (active or pending revocation), excluding the origin device itself,
-- plus the local device (if it's not the origin).
--
-- Remote device acknowledgment: peer_acks.last_applied_seq >= origin_seq.
-- Local device acknowledgment: sync_cursors.last_seq >= origin_seq.
--
-- A row is collectible only when NO required device is missing the
-- acknowledgment. See data/PLAN.md Step 29.7.
DELETE FROM change_log
WHERE origin_device_id IS NOT NULL
  AND origin_seq IS NOT NULL
  AND NOT EXISTS (
      SELECT 1
      FROM trusted_devices td
      WHERE td.space_id = change_log.space_id
        AND td.device_id != change_log.origin_device_id
        AND COALESCE(
            (SELECT pa.last_applied_seq
             FROM peer_acks pa
             WHERE pa.space_id = td.space_id
               AND pa.consuming_device_id = td.device_id
               AND pa.origin_device_id = change_log.origin_device_id),
            0
        ) < change_log.origin_seq
  )
  AND NOT EXISTS (
      SELECT 1
      WHERE change_log.origin_device_id !=
            (SELECT value FROM app_settings WHERE key = 'device_id')
        AND COALESCE(
            (SELECT sc.last_seq
             FROM sync_cursors sc
             WHERE sc.space_id = change_log.space_id
               AND sc.peer_device_id = change_log.origin_device_id),
            0
        ) < change_log.origin_seq
  )
