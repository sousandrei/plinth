SELECT dug.device_id AS "device_id!: String"
FROM device_user_grants dug
WHERE dug.space_id = ?1
  AND dug.user_id = ?2
  AND NOT EXISTS (
      SELECT 1
      FROM device_user_grants other
      WHERE other.space_id = dug.space_id
        AND other.device_id = dug.device_id
        AND other.user_id != dug.user_id
  )
