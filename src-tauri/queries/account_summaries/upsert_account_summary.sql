INSERT INTO account_summaries (month, account_id, balance)
VALUES (?1, ?2, ?3)
ON CONFLICT(month, account_id) DO UPDATE SET balance = excluded.balance
