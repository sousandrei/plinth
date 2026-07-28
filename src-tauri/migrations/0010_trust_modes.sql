-- V010: Separate Trust Modes (Step 30.4).
--
-- Replaces `space_devices.sync_enabled` (a boolean) with `trust_mode`
-- (a TEXT enum with three values: 'active', 'revoking',
-- 'revocation_only'). A grant's lifecycle is now expressible as a state
-- machine rather than a flag:
--
--   active            normal ping + bidirectional sync
--   revoking          local fence outbound while revocation propagates
--                     (the change_log row carrying this transition IS
--                     the revocation record)
--   revocation_only   peer may not participate in normal sync at all
--
-- The 'revoked' state is *not* a row value: revocation completes by
-- deleting the space_devices row entirely, which removes the cert from
-- the TLS trust set automatically. The durable record lives in the
-- change_log (every transition is captured) plus the now-dropped
-- `evicted_devices` table, which is no longer needed because Step 30.4
-- denies TLS access for fully-revoked peers — there's nothing to ship
-- to them any more.
--
-- Existing rows migrate: sync_enabled = 1  -> 'active'
--                       sync_enabled = 0  -> 'revoking'

-- ---------------------------------------------------------------------------
-- 1. Add the trust_mode column.
-- ---------------------------------------------------------------------------

ALTER TABLE space_devices
    ADD COLUMN trust_mode TEXT NOT NULL
        DEFAULT 'active'
        CHECK (trust_mode IN ('active', 'revoking', 'revocation_only'));

-- ---------------------------------------------------------------------------
-- 2. Backfill: any legacy `sync_enabled = 0` row is a pending revocation.
-- ---------------------------------------------------------------------------

UPDATE space_devices
SET trust_mode = 'revoking'
WHERE sync_enabled = 0;

-- ---------------------------------------------------------------------------
-- 3. Drop sync_enabled. SQLite 3.35+ supports ALTER TABLE DROP COLUMN.
--
--    Before this we have to drop the old triggers that reference
--    `new.sync_enabled`, since SQLite rejects DROP COLUMN when any
--    trigger body mentions the column.
-- ---------------------------------------------------------------------------

DROP TRIGGER IF EXISTS change_log_space_devices_ai;
DROP TRIGGER IF EXISTS change_log_space_devices_au;

ALTER TABLE space_devices DROP COLUMN sync_enabled;

-- ---------------------------------------------------------------------------
-- 4. Drop evicted_devices. With Step 30.4, a revoked peer has no
--    space_devices row, so resolve_peer will not find them — TLS
--    handshake succeeds only for peers with at least one trust_mode
--    grant, which by construction excludes revoked ones.
-- ---------------------------------------------------------------------------

DROP TABLE IF EXISTS evicted_devices;

-- ---------------------------------------------------------------------------
-- 5. Recreate the change_log triggers, now serializing `trust_mode`.
--    The original 0008 triggers are gone (dropped above); these
--    replace them. The Rust SpaceDevicePayload struct matches.
-- ---------------------------------------------------------------------------

CREATE TRIGGER IF NOT EXISTS change_log_space_devices_ai
AFTER INSERT ON space_devices
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'space_devices', new.space_id || ':' || new.device_id, 'insert',
        json_object(
            'space_id', new.space_id,
            'device_id', new.device_id,
            'trust_mode', new.trust_mode,
            'paired_at', new.paired_at
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        ),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq')
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_space_devices_au
AFTER UPDATE ON space_devices
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'space_devices', new.space_id || ':' || new.device_id, 'update',
        json_object(
            'space_id', new.space_id,
            'device_id', new.device_id,
            'trust_mode', new.trust_mode,
            'paired_at', new.paired_at
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        ),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq')
    );
END;

-- The DELETE trigger is unchanged in shape (no payload), but we recreate
-- it via DROP/CREATE to make the migration idempotent and to keep the
-- trigger set self-consistent.
DROP TRIGGER IF EXISTS change_log_space_devices_ad;
CREATE TRIGGER IF NOT EXISTS change_log_space_devices_ad
AFTER DELETE ON space_devices
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'space_devices', old.space_id || ':' || old.device_id, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        ),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq')
    );
END;
