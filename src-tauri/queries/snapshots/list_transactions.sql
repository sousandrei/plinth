-- Snapshot: list every transaction in this space, via account_id → accounts.
SELECT t.id, t.booking_date, t.value_date, t.reference, t.text,
       t.currency, t.amount, t.balance, t.approved, t.note,
       t.category, t.account_id
FROM transactions t
JOIN accounts a ON a.id = t.account_id
WHERE a.space_id = ?1
ORDER BY t.booking_date ASC, t.id ASC