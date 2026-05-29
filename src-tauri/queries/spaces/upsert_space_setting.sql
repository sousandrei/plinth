INSERT INTO space_settings (space_id, key, value)
VALUES (?1, ?2, ?3)
ON CONFLICT (space_id, key) DO UPDATE SET value = excluded.value
