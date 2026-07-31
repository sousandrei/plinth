-- V006: Lamport revisions — maintain row_winners for local writes.
--
-- An AFTER INSERT trigger on change_log upserts row_winners whenever a
-- local write creates a change_log entry. The trigger is guarded by
-- applying_remote so it does not fire during remote apply (the apply
-- code handles row_winners explicitly for remote changes).
--
-- The comparison uses (origin_seq, origin_device_id) — higher seq wins,
-- tiebreaker: higher origin_device_id wins (matching the bootstrap
-- ORDER BY in migration 0004). See data/PLAN.md Step 29.3.

CREATE TRIGGER IF NOT EXISTS change_log_winner_upsert
AFTER INSERT ON change_log
FOR EACH ROW
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    INSERT INTO row_winners (space_id, table_name, row_id, winning_seq, winning_origin, deleted)
    VALUES (
        NEW.space_id, NEW.table_name, NEW.row_id,
        COALESCE(NEW.origin_seq, NEW.seq),
        COALESCE(NEW.origin_device_id, NEW.device_id),
        CASE WHEN NEW.operation = 'delete' THEN 1 ELSE 0 END
    )
    ON CONFLICT (space_id, table_name, row_id) DO UPDATE
    SET winning_seq = excluded.winning_seq,
        winning_origin = excluded.winning_origin,
        deleted = excluded.deleted
    WHERE excluded.winning_seq > row_winners.winning_seq
       OR (excluded.winning_seq = row_winners.winning_seq
           AND excluded.winning_origin > row_winners.winning_origin);
END;
