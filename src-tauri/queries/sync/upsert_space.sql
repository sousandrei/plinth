INSERT INTO spaces (id, name, created_at, updated_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT (id) DO UPDATE SET
    name       = excluded.name,
    updated_at = excluded.updated_at
