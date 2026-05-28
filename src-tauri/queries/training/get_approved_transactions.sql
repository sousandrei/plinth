SELECT t.text, t.amount, t.booking_date, t.category AS "category!"
FROM transactions t
JOIN accounts a ON t.account_id = a.id
WHERE a.user_id = ?1
  AND t.approved = 1
  AND t.category IS NOT NULL
  AND t.category != ''
