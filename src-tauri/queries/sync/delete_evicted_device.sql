DELETE FROM evicted_devices
WHERE space_id = ?1 AND device_id = ?2;
