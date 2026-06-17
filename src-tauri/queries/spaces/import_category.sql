INSERT INTO categories (id, name, color, space_id)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(id) DO UPDATE SET
    name   = excluded.name,
    color  = excluded.color,
    space_id = excluded.space_id
