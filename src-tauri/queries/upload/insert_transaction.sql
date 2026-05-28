INSERT INTO transactions (
    id, booking_date, value_date, reference, text,
    currency, amount, balance, approved, note, category, account_id
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, '', ?9, ?10)
ON CONFLICT(id) DO NOTHING
