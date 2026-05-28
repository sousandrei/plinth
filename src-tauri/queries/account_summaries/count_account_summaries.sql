SELECT COUNT(*) AS "n!: i64"
FROM account_summaries s
INNER JOIN accounts a ON a.id = s.account_id
WHERE a.user_id = ?
