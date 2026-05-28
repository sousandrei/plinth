SELECT id, name, (pin_hash IS NOT NULL) AS "has_pin!: bool", created_at, updated_at
FROM users
ORDER BY created_at ASC
