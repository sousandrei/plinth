SELECT
    device_id AS "device_id!",
    cert_pem  AS "cert_pem!"
FROM trusted_devices
WHERE sync_enabled = 1
