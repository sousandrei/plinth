SELECT id AS "id!", name AS "name!", pin_hash, created_at AS "created_at!", updated_at AS "updated_at!"
FROM users
WHERE id = ?1
