INSERT INTO transactions (id, booking_date, value_date, reference, text, currency,
                          amount, balance, approved, note, category, account_id)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
ON CONFLICT(id) DO UPDATE SET
    booking_date = excluded.booking_date,
    value_date   = excluded.value_date,
    reference    = excluded.reference,
    text         = excluded.text,
    currency     = excluded.currency,
    amount       = excluded.amount,
    balance      = excluded.balance,
    approved     = excluded.approved,
    note         = excluded.note,
    category     = excluded.category,
    account_id   = excluded.account_id
