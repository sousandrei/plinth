-- Pairing snapshot: list every account in a space with its space_id.
-- The export_accounts.sql query doesn't include space_id, so this is a
-- pairing-specific variant.
SELECT id, name, currency, account_type, account_source, color, space_id
FROM accounts
WHERE space_id = ?1
ORDER BY name ASC