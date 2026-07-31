-- Insert a quarantined device row. Used when a cert fails ingress
-- validation (parse error, SAN mismatch, fingerprint collision,
-- device_id/fingerprint conflict). The caller passes the reason as a
-- short human-readable string; the row is not surfaced to other
-- devices via the change_log.
INSERT INTO quarantined_devices (space_id, claimed_device_id, fingerprint, cert_pem, reason)
VALUES (?1, ?2, ?3, ?4, ?5);
