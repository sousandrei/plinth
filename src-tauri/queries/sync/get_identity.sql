-- Read this device's persistent identity (cert + key + device_id) at once.
-- All three rows are initialized at first launch by db::init_sync_settings
-- and lazily filled by sync::identity::ensure_identity on first pairing.
SELECT key AS "key!", value AS "value!"
FROM app_settings
WHERE key IN ('device_id', 'device_cert_pem', 'device_key_pem')
