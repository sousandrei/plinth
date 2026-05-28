SELECT id, name, currency, account_type, account_source, user_id
FROM accounts
WHERE user_id = ?
ORDER BY name ASC
