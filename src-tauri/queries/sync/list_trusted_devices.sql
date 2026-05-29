SELECT
    id           AS "id!",
    space_id     AS "space_id!",
    device_id    AS "device_id!",
    display_name AS "display_name!",
    sync_enabled AS "sync_enabled!",
    paired_at    AS "paired_at!"
FROM trusted_devices
WHERE space_id = ?1
ORDER BY paired_at
