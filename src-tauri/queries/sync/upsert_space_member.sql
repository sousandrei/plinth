INSERT INTO space_members (space_id, user_id, role)
VALUES (?1, ?2, ?3)
ON CONFLICT (space_id, user_id) DO UPDATE SET role = excluded.role
