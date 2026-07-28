-- List every device's certificate with the spaces they have a grant in.
-- Used by resolve_peer to build a PeerIdentity from a TLS cert.
SELECT
    sd.space_id   AS "space_id!: String",
    sd.device_id  AS "device_id!: String",
    d.cert_pem    AS "cert_pem!: String"
FROM space_devices sd
JOIN devices d ON d.device_id = sd.device_id
INNER JOIN spaces s ON s.id = sd.space_id
UNION
SELECT
    space_id   AS "space_id!",
    device_id  AS "device_id!",
    cert_pem   AS "cert_pem!"
FROM evicted_devices ed
INNER JOIN spaces s ON s.id = ed.space_id
