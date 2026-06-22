-- Snapshot: list every account_summary in this space, via account_id → accounts.
SELECT s.month, s.account_id, s.balance
FROM account_summaries s
JOIN accounts a ON a.id = s.account_id
WHERE a.space_id = ?1
ORDER BY s.month ASC, s.account_id ASC