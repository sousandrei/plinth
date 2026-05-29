-- Most recent change_log entry for a given (table, row), excluding
-- entries authored by the incoming peer. Used by the conflict
-- detector to compare `changed_at` and `device_id` for the
-- 60-second tolerance window check.
--
-- Ordering by changed_at DESC then seq DESC handles same-second
-- writes deterministically — within one device the seq is monotonic.
SELECT changed_at, device_id
FROM change_log
WHERE table_name = ?1
  AND row_id     = ?2
  AND device_id != ?3
ORDER BY changed_at DESC, seq DESC
LIMIT 1;
