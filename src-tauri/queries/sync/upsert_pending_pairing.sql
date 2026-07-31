INSERT INTO pending_pairings (
    space_id,
    device_id,
    display_name,
    cert_pem,
    created_at
)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (space_id, device_id) DO UPDATE SET
    display_name = excluded.display_name,
    cert_pem = excluded.cert_pem,
    created_at = excluded.created_at;
