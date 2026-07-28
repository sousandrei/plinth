-- Upsert the winning revision for a logical row. The WHERE clause
-- ensures a losing revision cannot dethrone the existing winner.
-- Comparison: higher winning_seq wins; tiebreaker: higher winning_origin
-- (origin_device_id). Matches the bootstrap ORDER BY in migration 0004
-- and the change_log_winner_upsert trigger in migration 0006.
-- See data/PLAN.md Step 29.3.
INSERT INTO row_winners (space_id, table_name, row_id, winning_seq, winning_origin, deleted)
VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT (space_id, table_name, row_id) DO UPDATE
SET winning_seq = excluded.winning_seq,
    winning_origin = excluded.winning_origin,
    deleted = excluded.deleted
WHERE excluded.winning_seq > row_winners.winning_seq
   OR (excluded.winning_seq = row_winners.winning_seq
       AND excluded.winning_origin > row_winners.winning_origin)
