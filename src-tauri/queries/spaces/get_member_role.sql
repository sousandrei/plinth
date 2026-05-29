SELECT role AS "role!: String"
FROM space_members
WHERE space_id = ?1 AND user_id = ?2
