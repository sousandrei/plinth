-- The local device's SHA-256 fingerprint, hex-encoded. Used by
-- mDNS discovery to advertise the fingerprint so peers can
-- pre-validate the cert before opening the TLS connection. Returns
-- `None` if the local cert isn't yet in the `devices` table (cold
-- start; discovery just won't advertise the fingerprint).
SELECT fingerprint AS "fingerprint!: String"
FROM devices
WHERE device_id = ?1
LIMIT 1;
