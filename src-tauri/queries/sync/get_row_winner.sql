-- Get the current winning revision for a logical row, if any.
-- Used by the remote-apply path to compare (origin_seq, origin_device_id)
-- before materializing the row body. See data/PLAN.md Step 29.3.
SELECT winning_seq, winning_origin, deleted
FROM row_winners
WHERE space_id = ?1 AND table_name = ?2 AND row_id = ?3
