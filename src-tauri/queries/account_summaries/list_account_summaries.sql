SELECT s.month AS "month!: String",
       s.account_id AS "account_id!: String",
       s.balance AS "balance!: i64"
FROM account_summaries s
INNER JOIN accounts a ON a.id = s.account_id
WHERE a.space_id = ?
ORDER BY s.month DESC, a.name ASC
LIMIT ? OFFSET ?
