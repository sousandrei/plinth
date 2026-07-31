-- Snapshot: list materialized row revisions and deletion tombstones for a space.
SELECT space_id, table_name, row_id, winning_seq, winning_origin, deleted
FROM row_winners
WHERE space_id = ?1
ORDER BY table_name ASC, row_id ASC
