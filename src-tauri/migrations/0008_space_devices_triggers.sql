-- V008: Create change_log triggers for space_devices (replaces
-- trusted_devices triggers dropped by migration 0007). The devices
-- table itself is not synced via change_log — device identities are
-- established during pairing, not via change_log exchange.
-- See data/PLAN.md Step 30.1.

-- ---------------------------------------------------------------------------
-- space_devices triggers — fire on INSERT, UPDATE, DELETE.
-- row_id format: space_id || ':' || device_id (composite key).
-- ---------------------------------------------------------------------------

DROP TRIGGER IF EXISTS change_log_space_devices_ai;
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
            'sync_enabled', new.sync_enabled,
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

DROP TRIGGER IF EXISTS change_log_space_devices_au;
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
            'sync_enabled', new.sync_enabled,
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
