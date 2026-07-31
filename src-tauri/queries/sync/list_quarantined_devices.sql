-- List every quarantined device, newest first. Returned to the UI
-- so the user can review certs that failed ingress validation.
-- Step 30.2 — see data/PLAN.md.
SELECT
    id              AS "id!: i64",
    space_id        AS "space_id?: String",
    claimed_device_id AS "claimed_device_id!: String",
    fingerprint     AS "fingerprint?: String",
    cert_pem        AS "cert_pem!: String",
    reason          AS "reason!: String",
    quarantined_at  AS "quarantined_at!: String"
FROM quarantined_devices
ORDER BY id DESC;
