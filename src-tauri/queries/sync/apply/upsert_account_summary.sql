-- Byte-identical today to `queries/account_summaries/upsert_account_summary.sql`,
-- intentionally duplicated. App queries enforce caller-side invariants
-- and can evolve with UI requirements; apply queries must stay bit-stable
-- across versions so LWW semantics never silently shift. The full set
-- of statements that run under `applying_as_device` lives in this
-- directory and nowhere else.
INSERT INTO account_summaries (month, account_id, balance)
VALUES (?1, ?2, ?3)
ON CONFLICT(month, account_id) DO UPDATE SET
    balance = excluded.balance;
