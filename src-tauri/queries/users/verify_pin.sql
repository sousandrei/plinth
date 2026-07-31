SELECT c.pin_hash
FROM users u
LEFT JOIN local_user_credentials c ON c.user_id = u.id
WHERE u.id = ?
