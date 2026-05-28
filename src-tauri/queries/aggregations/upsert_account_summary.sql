INSERT INTO account_summaries (month, account_id, balance)
VALUES (?, ?, ?)
ON CONFLICT (month, account_id) DO UPDATE SET balance = excluded.balance
