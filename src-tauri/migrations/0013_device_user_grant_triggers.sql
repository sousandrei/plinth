-- V013: Replicate device-user relationships through the change log.

CREATE TRIGGER IF NOT EXISTS change_log_device_user_grants_ai
AFTER INSERT ON device_user_grants
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'device_user_grants',
        new.space_id || ':' || new.device_id || ':' || new.user_id, 'insert',
        json_object(
            'space_id', new.space_id,
            'device_id', new.device_id,
            'user_id', new.user_id,
            'granted_at', new.granted_at
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

CREATE TRIGGER IF NOT EXISTS change_log_device_user_grants_au
AFTER UPDATE ON device_user_grants
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'device_user_grants',
        new.space_id || ':' || new.device_id || ':' || new.user_id, 'update',
        json_object(
            'space_id', new.space_id,
            'device_id', new.device_id,
            'user_id', new.user_id,
            'granted_at', new.granted_at
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

CREATE TRIGGER IF NOT EXISTS change_log_device_user_grants_ad
AFTER DELETE ON device_user_grants
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'device_user_grants',
        old.space_id || ':' || old.device_id || ':' || old.user_id, 'delete', NULL,
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
