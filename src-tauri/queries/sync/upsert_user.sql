INSERT INTO users (id, name, pin_hash, created_at, updated_at)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (id) DO UPDATE SET
    name       = excluded.name,
    pin_hash   = excluded.pin_hash,
    updated_at = excluded.updated_at
