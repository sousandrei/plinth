-- V009: Quarantined certs — Step 30.2.
--
-- Certs that fail ingress validation (parse error, SAN mismatch,
-- fingerprint collision, device_id conflict) are recorded here
-- instead of being persisted to `devices`. The UI surfaces this
-- table so the user can see which peers presented bad certificates
-- without valid peers in unrelated spaces being affected.
--
-- One row per (claimed_device_id, fingerprint) pair. The fingerprint
-- is `sha256(der)` when we got far enough to compute it; NULL for
-- PEMs that failed to parse. The reason captures the validation
-- failure so the UI can group / explain them.

CREATE TABLE IF NOT EXISTS quarantined_devices (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    space_id        TEXT,
    claimed_device_id TEXT NOT NULL,
    fingerprint     TEXT,
    cert_pem        TEXT NOT NULL,
    reason          TEXT NOT NULL,
    quarantined_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_quarantined_devices_space
    ON quarantined_devices (space_id);

CREATE INDEX IF NOT EXISTS idx_quarantined_devices_fingerprint
    ON quarantined_devices (fingerprint);
