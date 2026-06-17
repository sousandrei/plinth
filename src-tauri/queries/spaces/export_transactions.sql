SELECT t.id           AS "id!: String",
       t.booking_date AS "booking_date!: String",
       t.value_date   AS "value_date!: String",
       t.reference    AS "reference!: String",
       t.text         AS "text!: String",
       t.currency     AS "currency!: String",
       t.amount       AS "amount!: i64",
       t.balance      AS "balance!: i64",
       t.approved     AS "approved!: i64",
       t.note         AS "note!: String",
       COALESCE(t.category, '') AS "category!: String",
       t.account_id   AS "account_id!: String"
FROM transactions t
INNER JOIN accounts a ON a.id = t.account_id
WHERE a.space_id = ?1
ORDER BY t.booking_date DESC, t.value_date DESC
