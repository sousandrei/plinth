SELECT id, name, currency, account_type, account_source, color
FROM accounts
WHERE space_id = ?1
ORDER BY name ASC
