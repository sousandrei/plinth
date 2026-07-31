SELECT space_id, device_id, user_id, granted_at
FROM device_user_grants
WHERE space_id = ?1
ORDER BY device_id, user_id
