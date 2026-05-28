INSERT INTO accounts (id, name, currency, account_type, account_source, user_id)
VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(id) DO NOTHING
