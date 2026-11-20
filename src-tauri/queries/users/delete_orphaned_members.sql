DELETE FROM users
WHERE id IN (SELECT value FROM json_each(?1))
  AND NOT EXISTS (SELECT 1 FROM space_members WHERE user_id = users.id)
