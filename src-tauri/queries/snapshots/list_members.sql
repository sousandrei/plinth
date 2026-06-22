-- Snapshot: list every member of a space.
SELECT space_id, user_id, role, joined_at
FROM space_members
WHERE space_id = ?1
ORDER BY joined_at ASC