-- Upsert a device row by device_id. Used during pairing and on first
-- launch to register the local device's certificate. The DER and
-- SHA-256 fingerprint are populated from the validated cert so that
-- `cert_match::resolve_peer` can use them for fast equality checks
-- (Step 30.2). A re-issued cert for the same device_id is allowed to
-- update cert_pem / cert_der / fingerprint — the caller is
-- responsible for verifying this is not an unexpected change
-- (Validator::check_against_existing) before calling this upsert.
INSERT INTO devices (device_id, cert_pem, cert_der, fingerprint, display_name)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (device_id) DO UPDATE
SET cert_pem = excluded.cert_pem,
    cert_der = excluded.cert_der,
    fingerprint = excluded.fingerprint,
    display_name = excluded.display_name
