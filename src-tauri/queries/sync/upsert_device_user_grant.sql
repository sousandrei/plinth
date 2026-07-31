INSERT INTO device_user_grants (space_id, device_id, user_id, granted_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT (space_id, device_id, user_id) DO UPDATE SET
    granted_at = excluded.granted_at
