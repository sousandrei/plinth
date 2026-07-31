SELECT user_id, granted_at
FROM device_user_grants
WHERE space_id = ?1
  AND device_id = ?2
ORDER BY user_id
