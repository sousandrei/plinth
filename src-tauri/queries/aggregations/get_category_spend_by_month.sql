SELECT strftime('%Y-%m', t.value_date) AS "month!: String",
       t.category                      AS "category!: String",
       SUM(t.amount)                   AS "amount!: i64"
FROM transactions t
INNER JOIN accounts a ON a.id = t.account_id
WHERE a.space_id = ?
  AND t.approved = 1
  AND t.category IS NOT NULL
GROUP BY "month!: String", t.category
ORDER BY "month!: String" ASC
