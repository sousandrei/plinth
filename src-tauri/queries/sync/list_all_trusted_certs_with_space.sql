SELECT
    space_id  AS "space_id!",
    device_id AS "device_id!",
    cert_pem  AS "cert_pem!"
FROM trusted_devices td
INNER JOIN spaces s ON s.id = td.space_id
UNION
SELECT
    space_id  AS "space_id!",
    device_id AS "device_id!",
    cert_pem  AS "cert_pem!"
FROM evicted_devices ed
INNER JOIN spaces s ON s.id = ed.space_id
