-- Delete a space_device grant during remote apply. Replaces
-- apply/delete_trusted_device.sql.
DELETE FROM space_devices WHERE space_id = ?1 AND device_id = ?2
