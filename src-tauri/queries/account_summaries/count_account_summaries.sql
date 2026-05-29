SELECT COUNT(*) AS "count!: i64"
FROM account_summaries s
INNER JOIN accounts a ON a.id = s.account_id
WHERE a.space_id = ?
