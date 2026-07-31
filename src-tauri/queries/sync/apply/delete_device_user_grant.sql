DELETE FROM device_user_grants
WHERE space_id = ?1
  AND device_id = ?2
  AND user_id = ?3
