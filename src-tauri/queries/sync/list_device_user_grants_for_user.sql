SELECT device_id, granted_at
FROM device_user_grants
WHERE space_id = ?1
  AND user_id = ?2
ORDER BY device_id
