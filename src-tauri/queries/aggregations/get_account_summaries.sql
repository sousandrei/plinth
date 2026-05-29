SELECT s.month, s.account_id, s.balance
FROM account_summaries s
INNER JOIN accounts a ON a.id = s.account_id
WHERE a.space_id = ?
ORDER BY s.month ASC
