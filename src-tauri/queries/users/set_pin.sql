INSERT INTO local_user_credentials (user_id, pin_hash, updated_at)
SELECT id, ?1, ?2
FROM users
WHERE id = ?3
ON CONFLICT (user_id) DO UPDATE SET
    pin_hash = excluded.pin_hash,
    updated_at = excluded.updated_at
