SELECT s.id, s.name, s.created_at, s.updated_at, sm.role AS "role!: String"
FROM spaces s
INNER JOIN space_members sm ON sm.space_id = s.id
WHERE sm.user_id = ?1
ORDER BY s.created_at ASC
