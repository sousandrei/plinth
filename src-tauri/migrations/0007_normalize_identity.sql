-- V007: Normalize installation identity — split trusted_devices into
-- devices (canonical identity + certificate) and space_devices (per-space
-- authorization). Migrate existing rows by logical (space_id, device_id).
-- See data/PLAN.md Step 30.1.

-- ---------------------------------------------------------------------------
-- 1. devices — one row per logical installation
-- ---------------------------------------------------------------------------
-- Canonical certificate identity. The device_id is the primary key so
-- the same installation is recognized across spaces. The cert_pem and
-- cert_der are stored for TLS verification (Step 30.3). The fingerprint
-- is SHA-256(der), hex-encoded, for fast lookup.

CREATE TABLE IF NOT EXISTS devices (
    device_id     TEXT PRIMARY KEY NOT NULL,
    cert_pem      TEXT NOT NULL,
    cert_der      BLOB,
    fingerprint   TEXT NOT NULL DEFAULT '',
    display_name  TEXT NOT NULL DEFAULT ''
);

-- ---------------------------------------------------------------------------
-- 2. space_devices — one row per (space, device) authorization
-- ---------------------------------------------------------------------------
-- Grants a device access to a space. Keyed by (space_id, device_id) so
-- random row IDs are eliminated from synchronization semantics (Step 30.1
-- verification: existing peers converge to one logical grant). The
-- winning_revision tracks the highest peer_acks.last_applied_seq the
-- device has acknowledged — used by future conflict resolution.

CREATE TABLE IF NOT EXISTS space_devices (
    space_id          TEXT NOT NULL REFERENCES spaces(id),
    device_id         TEXT NOT NULL REFERENCES devices(device_id),
    sync_enabled      INTEGER NOT NULL DEFAULT 1,
    paired_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    winning_revision  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (space_id, device_id)
);

-- No CASCADE: space deletion must run gc_orphan_space_devices first
-- to emit change_log entries for any pending peer. The GC pass then
-- removes the orphan space_devices rows before the space row itself
-- is deleted.

CREATE INDEX IF NOT EXISTS idx_space_devices_device
    ON space_devices (device_id);

-- ---------------------------------------------------------------------------
-- 3. Migrate from trusted_devices
-- ---------------------------------------------------------------------------
-- For each unique device_id across all trusted_devices rows, insert one
-- devices row. Use the first non-empty cert_pem seen for that device.
-- (If different rows for the same device have different certs, the
-- cert mismatch is a pre-existing bug; the migration uses the first one
-- and Step 30.2 will validate consistency.)

INSERT OR IGNORE INTO devices (device_id, cert_pem, display_name)
SELECT
    td.device_id,
    (SELECT cert_pem FROM trusted_devices t2
     WHERE t2.device_id = td.device_id AND t2.cert_pem != ''
     ORDER BY t2.paired_at LIMIT 1) AS cert_pem,
    (SELECT display_name FROM trusted_devices t2
     WHERE t2.device_id = td.device_id
     ORDER BY t2.paired_at LIMIT 1) AS display_name
FROM trusted_devices td
GROUP BY td.device_id;

-- For each unique (space_id, device_id) in trusted_devices, insert one
-- space_devices row. If different rows for the same key have different
-- sync_enabled values, use the most recent (max by paired_at).

INSERT OR IGNORE INTO space_devices (space_id, device_id, sync_enabled, paired_at)
SELECT
    space_id,
    device_id,
    sync_enabled,
    paired_at
FROM trusted_devices
GROUP BY space_id, device_id;

-- Resolve sync_enabled conflicts: if multiple trusted_devices rows for
-- the same (space_id, device_id) had different sync_enabled values, the
-- GROUP BY picked one arbitrarily. Promote to the most permissive (1)
-- if any row had sync_enabled = 1.

UPDATE space_devices
SET sync_enabled = 1
WHERE EXISTS (
    SELECT 1 FROM trusted_devices td
    WHERE td.space_id = space_devices.space_id
      AND td.device_id = space_devices.device_id
      AND td.sync_enabled = 1
);

-- ---------------------------------------------------------------------------
-- 4. Drop the old trusted_devices table
-- ---------------------------------------------------------------------------

DROP TABLE IF EXISTS trusted_devices;
