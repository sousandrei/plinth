INSERT INTO accounts (id, name, currency, account_type, account_source, color, space_id)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(id) DO UPDATE SET
    name           = excluded.name,
    currency       = excluded.currency,
    account_type   = excluded.account_type,
    account_source = excluded.account_source,
    color          = excluded.color,
    space_id       = excluded.space_id
