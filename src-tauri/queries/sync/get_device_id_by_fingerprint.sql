-- Look up the device_id currently associated with a fingerprint.
-- Returns at most one row. Used during cert ingress to detect
-- fingerprint collisions (same fingerprint, different device_id) and
-- device_id changes (same device_id, different fingerprint).
SELECT device_id AS "device_id!: String"
FROM devices
WHERE fingerprint = ?1
LIMIT 1;
