-- Byte-identical today to `queries/account_summaries/delete_account_summary.sql`,
-- intentionally duplicated. See `apply/upsert_account_summary.sql` for
-- the rationale (apply queries form a stable, single-surface audit
-- target independent of app-layer SQL evolution).
DELETE FROM account_summaries WHERE month = ?1 AND account_id = ?2;
