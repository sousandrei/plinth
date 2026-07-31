-- Revoke a device's access to a space.
DELETE FROM space_devices WHERE space_id = ?1 AND device_id = ?2
