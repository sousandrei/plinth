SELECT
    device_id AS "device_id!",
    cert_pem  AS "cert_pem!"
FROM trusted_devices
UNION
SELECT
    device_id AS "device_id!",
    cert_pem  AS "cert_pem!"
FROM evicted_devices
