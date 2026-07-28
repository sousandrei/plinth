-- List all space-device grants for a space, joined with device identity.
SELECT
    sd.space_id    AS "space_id!: String",
    sd.device_id   AS "device_id!: String",
    sd.sync_enabled AS "sync_enabled!: i64",
    sd.paired_at   AS "paired_at!: String",
    d.display_name AS "display_name!: String"
FROM space_devices sd
JOIN devices d ON d.device_id = sd.device_id
WHERE sd.space_id = ?1
ORDER BY sd.paired_at
