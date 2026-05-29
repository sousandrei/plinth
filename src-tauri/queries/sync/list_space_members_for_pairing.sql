SELECT sm.space_id AS "space_id!", sm.user_id AS "user_id!", sm.role AS "role!", sm.joined_at AS "joined_at!"
FROM space_members sm
WHERE sm.space_id = ?1
