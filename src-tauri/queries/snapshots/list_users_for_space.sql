-- Snapshot: list every user that is a member of this space.
SELECT u.id, u.name, u.pin_hash, u.created_at, u.updated_at
FROM users u
JOIN space_members sm ON sm.user_id = u.id
WHERE sm.space_id = ?1
ORDER BY u.name ASC