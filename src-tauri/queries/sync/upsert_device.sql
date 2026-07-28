-- Upsert a device row by device_id. Used during pairing and on first
-- launch to register the local device's certificate.
INSERT INTO devices (device_id, cert_pem, display_name)
VALUES (?1, ?2, ?3)
ON CONFLICT (device_id) DO UPDATE
SET cert_pem = excluded.cert_pem,
    display_name = excluded.display_name
