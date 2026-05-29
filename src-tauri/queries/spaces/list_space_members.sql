SELECT sm.user_id AS "user_id!: String", sm.role AS "role!: String", u.name AS "name!: String"
FROM space_members sm
INNER JOIN users u ON u.id = sm.user_id
WHERE sm.space_id = ?1
ORDER BY sm.joined_at ASC
