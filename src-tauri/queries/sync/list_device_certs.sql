-- List every device's certificate for TLS verification. Replaces
-- list_all_trusted_certs.sql from the trusted_devices era.
SELECT
    device_id  AS "device_id!: String",
    cert_pem   AS "cert_pem!: String"
FROM devices
