SELECT strftime('%Y-%m', t.value_date) AS "month!: String",
       t.account_id                    AS "account_id!: String",
       t.balance                       AS "balance!: i64"
FROM transactions t
INNER JOIN accounts a ON a.id = t.account_id
WHERE a.user_id = ?
  AND a.account_type IN ('checking', 'savings')
  AND t.value_date = (
      SELECT MAX(t2.value_date)
      FROM transactions t2
      WHERE t2.account_id = t.account_id
        AND strftime('%Y-%m', t2.value_date) = strftime('%Y-%m', t.value_date)
  )
GROUP BY strftime('%Y-%m', t.value_date), t.account_id
ORDER BY strftime('%Y-%m', t.value_date) ASC
