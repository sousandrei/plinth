-- Snapshot: load a single space row.
SELECT id, name, created_at, updated_at
FROM spaces
WHERE id = ?1
  AND deleted = 0