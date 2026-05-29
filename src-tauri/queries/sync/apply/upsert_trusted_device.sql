INSERT INTO trusted_devices (
    id, space_id, device_id, display_name, cert_pem, sync_enabled, paired_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(id) DO UPDATE SET
    space_id     = excluded.space_id,
    device_id    = excluded.device_id,
    display_name = excluded.display_name,
    cert_pem     = excluded.cert_pem,
    sync_enabled = excluded.sync_enabled,
    paired_at    = excluded.paired_at;
