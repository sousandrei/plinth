-- List device IDs that have a grant in a space.
SELECT device_id AS "device_id!: String"
FROM space_devices
WHERE space_id = ?1
ORDER BY paired_at
