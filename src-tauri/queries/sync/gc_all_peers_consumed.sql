-- 10.2 All-peers-consumed GC: delete change_log rows that every active
-- (sync_enabled = 1) trusted peer for the same space has already consumed,
-- i.e. their sync_cursors.last_seq >= the row's seq.
--
-- Guard: only GC rows where at least one trusted+enabled peer exists for
-- the space. With no peers the subquery would trivially match everything
-- and silently erase un-synced history.
--
-- A missing sync_cursors row (peer hasn't synced yet) is treated as
-- last_seq = -1 so those rows are never deleted. See data/PLAN.md §10.2.
DELETE FROM change_log
WHERE (
    SELECT COUNT(*)
    FROM trusted_devices td
    WHERE td.space_id  = change_log.space_id
      AND td.sync_enabled = 1
) > 0
AND NOT EXISTS (
    SELECT 1
    FROM trusted_devices td
    WHERE td.space_id  = change_log.space_id
      AND td.sync_enabled = 1
      AND COALESCE(
              (SELECT sc.last_seq
               FROM sync_cursors sc
               WHERE sc.space_id       = change_log.space_id
                 AND sc.peer_device_id = td.device_id),
              -1
          ) < change_log.seq
)
