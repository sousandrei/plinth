SELECT id, name, EXISTS (SELECT 1 FROM local_user_credentials c WHERE c.user_id = users.id) AS "has_pin!: bool", created_at, updated_at
FROM users
WHERE id = ?
