-- Look up the fingerprint currently stored for a device_id. Returns
-- at most one row; empty fingerprint if the row exists but the
-- fingerprint is unset (e.g. a stub row from a change_log-driven
-- space_devices insert). Used during cert ingress to detect
-- unexpected fingerprint changes.
SELECT fingerprint AS "fingerprint!: String"
FROM devices
WHERE device_id = ?1
LIMIT 1;
