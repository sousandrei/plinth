-- Snapshot: list device identities associated with space_devices grants for a space.
SELECT DISTINCT d.device_id, d.cert_pem, d.display_name
FROM devices d
JOIN space_devices sd ON sd.device_id = d.device_id
WHERE sd.space_id = ?1
ORDER BY d.device_id ASC
