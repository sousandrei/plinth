SELECT t.id,
       t.approved AS "approved!: bool",
       t.note AS "note!: String",
       t.category
FROM transactions t
JOIN accounts a ON a.id = t.account_id
WHERE t.id IN (SELECT value FROM json_each(?1)) AND a.space_id = ?2
