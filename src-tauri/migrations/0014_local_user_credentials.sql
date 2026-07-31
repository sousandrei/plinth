-- Keep authentication material local to each installation.
CREATE TABLE IF NOT EXISTS local_user_credentials (
    user_id    TEXT PRIMARY KEY NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pin_hash   TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO local_user_credentials (user_id, pin_hash, updated_at)
SELECT id, pin_hash, updated_at
FROM users
WHERE pin_hash IS NOT NULL
ON CONFLICT (user_id) DO UPDATE SET
    pin_hash = excluded.pin_hash,
    updated_at = excluded.updated_at;

DROP TRIGGER IF EXISTS change_log_space_members_ai;
DROP TRIGGER IF EXISTS change_log_space_members_au;
DROP TRIGGER IF EXISTS change_log_space_members_ad;
DROP TRIGGER IF EXISTS change_log_users_au;

UPDATE users SET pin_hash = NULL WHERE pin_hash IS NOT NULL;

CREATE TRIGGER change_log_space_members_ai
AFTER INSERT ON space_members
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'space_members', new.space_id || ':' || new.user_id, 'insert',
        json_object(
            'space_id', new.space_id, 'user_id', new.user_id,
            'role', new.role, 'joined_at', new.joined_at,
            'user', (
                SELECT json_object(
                    'id', u.id, 'name', u.name,
                    'created_at', u.created_at, 'updated_at', u.updated_at
                )
                FROM users u WHERE u.id = new.user_id
            )
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq')
    );
END;

CREATE TRIGGER change_log_space_members_au
AFTER UPDATE ON space_members
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'space_members', new.space_id || ':' || new.user_id, 'update',
        json_object(
            'space_id', new.space_id, 'user_id', new.user_id,
            'role', new.role, 'joined_at', new.joined_at,
            'user', (
                SELECT json_object(
                    'id', u.id, 'name', u.name,
                    'created_at', u.created_at, 'updated_at', u.updated_at
                )
                FROM users u WHERE u.id = new.user_id
            )
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq')
    );
END;

CREATE TRIGGER change_log_space_members_ad
AFTER DELETE ON space_members
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'space_members', old.space_id || ':' || old.user_id, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq')
    );
END;

CREATE TRIGGER change_log_users_au
AFTER UPDATE OF name, created_at, updated_at ON users
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    SELECT
        lower(hex(randomblob(16))),
        sm.space_id, 'space_members', sm.space_id || ':' || sm.user_id, 'update',
        json_object(
            'space_id', sm.space_id, 'user_id', sm.user_id,
            'role', sm.role, 'joined_at', sm.joined_at,
            'user', json_object(
                'id', new.id, 'name', new.name,
                'created_at', new.created_at, 'updated_at', new.updated_at
            )
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq') + 1,
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq') + 1
    FROM space_members sm WHERE sm.user_id = new.id;

    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq'
          AND EXISTS (SELECT 1 FROM space_members WHERE user_id = new.id);
END;
