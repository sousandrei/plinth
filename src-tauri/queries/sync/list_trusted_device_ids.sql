SELECT DISTINCT td.device_id AS "device_id!"
FROM trusted_devices td
INNER JOIN spaces s ON s.id = td.space_id
WHERE td.sync_enabled = 1
