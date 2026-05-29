SELECT COUNT(*) AS count
FROM transactions t
JOIN accounts a ON t.account_id = a.id
WHERE a.space_id = ?1
  AND t.approved = 1
  AND t.category IS NOT NULL
  AND t.category != ''
