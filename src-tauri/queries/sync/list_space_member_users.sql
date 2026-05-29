SELECT u.id AS "id!", u.name AS "name!", u.pin_hash, u.created_at AS "created_at!", u.updated_at AS "updated_at!"
FROM users u
INNER JOIN space_members sm ON sm.user_id = u.id
WHERE sm.space_id = ?1
