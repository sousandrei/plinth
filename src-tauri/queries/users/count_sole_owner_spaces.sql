SELECT s.name AS "space_name!: String"
FROM space_members sm
INNER JOIN spaces s ON s.id = sm.space_id
WHERE sm.user_id = ?1 AND sm.role = 'owner'
GROUP BY sm.space_id
HAVING COUNT(*) = 1
