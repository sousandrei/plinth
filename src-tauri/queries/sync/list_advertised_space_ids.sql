-- List device IDs that have a grant (sync_enabled = 1) in a space.
-- Used for mDNS service property advertisement.
SELECT DISTINCT sd.space_id AS "space_id!: String"
FROM space_devices sd
WHERE sd.sync_enabled = 1
