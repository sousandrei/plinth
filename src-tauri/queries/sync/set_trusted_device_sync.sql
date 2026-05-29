UPDATE trusted_devices
SET sync_enabled = ?3
WHERE space_id = ?1 AND id = ?2
