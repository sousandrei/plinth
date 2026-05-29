INSERT INTO space_members (space_id, user_id, role, joined_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(space_id, user_id) DO UPDATE SET
    role      = excluded.role,
    joined_at = excluded.joined_at;
