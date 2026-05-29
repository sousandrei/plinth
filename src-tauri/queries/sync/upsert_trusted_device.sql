INSERT INTO trusted_devices (id, space_id, device_id, display_name, cert_pem)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (space_id, device_id) DO UPDATE SET
    display_name = excluded.display_name,
    cert_pem     = excluded.cert_pem,
    sync_enabled = 1
