SELECT id, name, currency, account_type, account_source, color, space_id
FROM accounts
WHERE space_id = ?
ORDER BY name ASC
