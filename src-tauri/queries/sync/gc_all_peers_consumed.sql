-- Step 30.4: only `trust_mode = 'active'` grants count as consumers for
-- the all-peers-consumed GC. A `revoking` grant is a local fence: the
-- device may still ack rows, but it's about to lose access, so we
-- exclude it from the consumer set so its cursor advance doesn't
-- prematurely collect changes that other active peers haven't seen.
--
-- Guard: only GC rows where at least one active peer exists for the
-- space. With no active peers the subquery would trivially match
-- everything and silently erase un-synced history.
--
-- A missing sync_cursors row (peer hasn't synced yet) is treated as
-- last_seq = -1 so those rows are never deleted. See data/PLAN.md §10.2.
DELETE FROM change_log
WHERE (
    SELECT COUNT(*)
    FROM space_devices sd
    WHERE sd.space_id  = change_log.space_id
      AND sd.trust_mode = 'active'
) > 0
AND NOT EXISTS (
    SELECT 1
    FROM space_devices sd
    WHERE sd.space_id  = change_log.space_id
      AND sd.trust_mode = 'active'
      AND COALESCE(
              (SELECT sc.last_seq
               FROM sync_cursors sc
               WHERE sc.space_id       = change_log.space_id
                 AND sc.peer_device_id = sd.device_id),
              -1
          ) < change_log.seq
)
