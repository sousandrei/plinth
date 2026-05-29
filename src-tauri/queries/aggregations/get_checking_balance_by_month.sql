SELECT month                    AS "month!: String",
       account_id               AS "account_id!: String",
       balance                  AS "balance!: i64"
FROM (
    SELECT strftime('%Y-%m', t.value_date)                         AS month,
           t.account_id                                            AS account_id,
           t.balance                                               AS balance,
           MAX(t.value_date) OVER (
               PARTITION BY t.account_id, strftime('%Y-%m', t.value_date)
           )                                                       AS max_date,
           t.value_date
    FROM transactions t
    INNER JOIN accounts a ON a.id = t.account_id
    WHERE a.space_id = ?
      AND a.account_type IN ('checking', 'savings')
)
WHERE value_date = max_date
ORDER BY month ASC
