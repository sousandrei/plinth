-- V005: Suppress change_log triggers during remote apply and set
-- immutable origin columns on local writes. See data/PLAN.md Step 29.2.
--
-- Every change_log trigger is dropped and recreated with a WHEN guard
-- that checks `app_settings('applying_remote')`. When the apply path
-- sets that key to '1', triggers do not fire — no echo change_log row,
-- no local sync_seq bump. The apply code inserts the incoming change_log
-- row directly with the original (origin_device_id, origin_seq).
--
-- On local writes (applying_remote is unset), triggers fire normally and
-- set origin_device_id = device_id and origin_seq = seq, so local
-- changes have a self-consistent origin identity.
--
-- The auto-updated_at triggers (spaces_au, users_au) are also guarded so
-- they don't overwrite remote timestamps during apply.

-- ---------------------------------------------------------------------------
-- Drop all existing change_log triggers + auto-updated_at triggers
-- ---------------------------------------------------------------------------

DROP TRIGGER IF EXISTS change_log_spaces_ai;
DROP TRIGGER IF EXISTS change_log_spaces_au;
DROP TRIGGER IF EXISTS change_log_spaces_ad;
DROP TRIGGER IF EXISTS change_log_space_members_ai;
DROP TRIGGER IF EXISTS change_log_space_members_au;
DROP TRIGGER IF EXISTS change_log_space_members_ad;
DROP TRIGGER IF EXISTS change_log_users_au;
DROP TRIGGER IF EXISTS change_log_accounts_ai;
DROP TRIGGER IF EXISTS change_log_accounts_au;
DROP TRIGGER IF EXISTS change_log_accounts_ad;
DROP TRIGGER IF EXISTS change_log_categories_ai;
DROP TRIGGER IF EXISTS change_log_categories_au;
DROP TRIGGER IF EXISTS change_log_categories_ad;
DROP TRIGGER IF EXISTS change_log_transactions_ai;
DROP TRIGGER IF EXISTS change_log_transactions_au;
DROP TRIGGER IF EXISTS change_log_transactions_ad;
DROP TRIGGER IF EXISTS change_log_account_summaries_ai;
DROP TRIGGER IF EXISTS change_log_account_summaries_au;
DROP TRIGGER IF EXISTS change_log_account_summaries_ad;
DROP TRIGGER IF EXISTS change_log_space_settings_ai;
DROP TRIGGER IF EXISTS change_log_space_settings_au;
DROP TRIGGER IF EXISTS change_log_space_settings_ad;
DROP TRIGGER IF EXISTS change_log_trusted_devices_ai;
DROP TRIGGER IF EXISTS change_log_trusted_devices_au;
DROP TRIGGER IF EXISTS change_log_trusted_devices_ad;
DROP TRIGGER IF EXISTS change_log_model_versions_ai;
DROP TRIGGER IF EXISTS change_log_model_versions_au;
DROP TRIGGER IF EXISTS change_log_model_versions_ad;
DROP TRIGGER IF EXISTS spaces_au;
DROP TRIGGER IF EXISTS users_au;

-- ---------------------------------------------------------------------------
-- Recreate auto-updated_at triggers with applying_remote guard
-- ---------------------------------------------------------------------------

CREATE TRIGGER IF NOT EXISTS users_au
AFTER UPDATE ON users
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE users SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = new.id;
END;

CREATE TRIGGER IF NOT EXISTS spaces_au
AFTER UPDATE ON spaces
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE spaces SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = new.id;
END;

-- ---------------------------------------------------------------------------
-- Recreate all change_log triggers with applying_remote guard + origin cols
-- ---------------------------------------------------------------------------

-- spaces
CREATE TRIGGER IF NOT EXISTS change_log_spaces_ai
AFTER INSERT ON spaces
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.id, 'spaces', new.id, 'insert',
        json_object(
            'id', new.id, 'name', new.name,
            'created_at', new.created_at, 'updated_at', new.updated_at
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

CREATE TRIGGER IF NOT EXISTS change_log_spaces_au
AFTER UPDATE ON spaces
WHEN NEW.deleted = 0
  AND COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.id, 'spaces', new.id, 'update',
        json_object(
            'id', new.id, 'name', new.name,
            'created_at', new.created_at, 'updated_at', new.updated_at
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

CREATE TRIGGER IF NOT EXISTS change_log_spaces_ad
AFTER DELETE ON spaces
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.id, 'spaces', old.id, 'delete', NULL,
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

-- space_members
CREATE TRIGGER IF NOT EXISTS change_log_space_members_ai
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
                    'id', u.id, 'name', u.name, 'pin_hash', u.pin_hash,
                    'created_at', u.created_at, 'updated_at', u.updated_at
                )
                FROM users u WHERE u.id = new.user_id
            )
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

CREATE TRIGGER IF NOT EXISTS change_log_space_members_au
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
                    'id', u.id, 'name', u.name, 'pin_hash', u.pin_hash,
                    'created_at', u.created_at, 'updated_at', u.updated_at
                )
                FROM users u WHERE u.id = new.user_id
            )
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

CREATE TRIGGER IF NOT EXISTS change_log_space_members_ad
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

-- users (synthetic space_members update for each space the user belongs to)
CREATE TRIGGER IF NOT EXISTS change_log_users_au
AFTER UPDATE ON users
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
                'id', new.id, 'name', new.name, 'pin_hash', new.pin_hash,
                'created_at', new.created_at, 'updated_at', new.updated_at
            )
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq') + 1,
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        ),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq') + 1
    FROM space_members sm WHERE sm.user_id = new.id;

    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq'
          AND EXISTS (SELECT 1 FROM space_members WHERE user_id = new.id);
END;

-- accounts
CREATE TRIGGER IF NOT EXISTS change_log_accounts_ai
AFTER INSERT ON accounts
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'accounts', new.id, 'insert',
        json_object(
            'id', new.id, 'name', new.name, 'currency', new.currency,
            'account_type', new.account_type, 'account_source', new.account_source,
            'color', new.color, 'space_id', new.space_id
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

CREATE TRIGGER IF NOT EXISTS change_log_accounts_au
AFTER UPDATE ON accounts
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'accounts', new.id, 'update',
        json_object(
            'id', new.id, 'name', new.name, 'currency', new.currency,
            'account_type', new.account_type, 'account_source', new.account_source,
            'color', new.color, 'space_id', new.space_id
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

CREATE TRIGGER IF NOT EXISTS change_log_accounts_ad
AFTER DELETE ON accounts
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'accounts', old.id, 'delete', NULL,
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

-- categories
CREATE TRIGGER IF NOT EXISTS change_log_categories_ai
AFTER INSERT ON categories
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'categories', new.id, 'insert',
        json_object('id', new.id, 'name', new.name, 'color', new.color, 'space_id', new.space_id),
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

CREATE TRIGGER IF NOT EXISTS change_log_categories_au
AFTER UPDATE ON categories
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'categories', new.id, 'update',
        json_object('id', new.id, 'name', new.name, 'color', new.color, 'space_id', new.space_id),
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

CREATE TRIGGER IF NOT EXISTS change_log_categories_ad
AFTER DELETE ON categories
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'categories', old.id, 'delete', NULL,
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

-- transactions
CREATE TRIGGER IF NOT EXISTS change_log_transactions_ai
AFTER INSERT ON transactions
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = new.account_id),
        'transactions', new.id, 'insert',
        json_object(
            'id', new.id, 'booking_date', new.booking_date, 'value_date', new.value_date,
            'reference', new.reference, 'text', new.text, 'currency', new.currency,
            'amount', new.amount, 'balance', new.balance, 'approved', new.approved,
            'note', new.note, 'category', new.category, 'account_id', new.account_id
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

CREATE TRIGGER IF NOT EXISTS change_log_transactions_au
AFTER UPDATE ON transactions
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = new.account_id),
        'transactions', new.id, 'update',
        json_object(
            'id', new.id, 'booking_date', new.booking_date, 'value_date', new.value_date,
            'reference', new.reference, 'text', new.text, 'currency', new.currency,
            'amount', new.amount, 'balance', new.balance, 'approved', new.approved,
            'note', new.note, 'category', new.category, 'account_id', new.account_id
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

CREATE TRIGGER IF NOT EXISTS change_log_transactions_ad
AFTER DELETE ON transactions
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = old.account_id),
        'transactions', old.id, 'delete', NULL,
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

-- account_summaries
CREATE TRIGGER IF NOT EXISTS change_log_account_summaries_ai
AFTER INSERT ON account_summaries
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = new.account_id),
        'account_summaries', new.account_id || ':' || new.month, 'insert',
        json_object('month', new.month, 'account_id', new.account_id, 'balance', new.balance),
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

CREATE TRIGGER IF NOT EXISTS change_log_account_summaries_au
AFTER UPDATE ON account_summaries
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = new.account_id),
        'account_summaries', new.account_id || ':' || new.month, 'update',
        json_object('month', new.month, 'account_id', new.account_id, 'balance', new.balance),
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

CREATE TRIGGER IF NOT EXISTS change_log_account_summaries_ad
AFTER DELETE ON account_summaries
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = old.account_id),
        'account_summaries', old.account_id || ':' || old.month, 'delete', NULL,
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

-- space_settings
CREATE TRIGGER IF NOT EXISTS change_log_space_settings_ai
AFTER INSERT ON space_settings
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'space_settings', new.space_id || ':' || new.key, 'insert',
        json_object('space_id', new.space_id, 'key', new.key, 'value', new.value),
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

CREATE TRIGGER IF NOT EXISTS change_log_space_settings_au
AFTER UPDATE ON space_settings
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'space_settings', new.space_id || ':' || new.key, 'update',
        json_object('space_id', new.space_id, 'key', new.key, 'value', new.value),
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

CREATE TRIGGER IF NOT EXISTS change_log_space_settings_ad
AFTER DELETE ON space_settings
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'space_settings', old.space_id || ':' || old.key, 'delete', NULL,
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

-- trusted_devices
CREATE TRIGGER IF NOT EXISTS change_log_trusted_devices_ai
AFTER INSERT ON trusted_devices
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'trusted_devices', new.id, 'insert',
        json_object(
            'id', new.id, 'space_id', new.space_id, 'device_id', new.device_id,
            'display_name', new.display_name, 'cert_pem', new.cert_pem,
            'sync_enabled', new.sync_enabled, 'paired_at', new.paired_at
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

CREATE TRIGGER IF NOT EXISTS change_log_trusted_devices_au
AFTER UPDATE ON trusted_devices
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'trusted_devices', new.id, 'update',
        json_object(
            'id', new.id, 'space_id', new.space_id, 'device_id', new.device_id,
            'display_name', new.display_name, 'cert_pem', new.cert_pem,
            'sync_enabled', new.sync_enabled, 'paired_at', new.paired_at
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

CREATE TRIGGER IF NOT EXISTS change_log_trusted_devices_ad
AFTER DELETE ON trusted_devices
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'trusted_devices', old.id, 'delete', NULL,
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

-- model_versions
CREATE TRIGGER IF NOT EXISTS change_log_model_versions_ai
AFTER INSERT ON model_versions
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'model_versions', new.space_id || ':' || CAST(new.version AS TEXT), 'insert',
        json_object(
            'space_id', new.space_id, 'version', new.version,
            'weights_md5', new.weights_md5, 'card_md5', new.card_md5,
            'trained_at', new.trained_at
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

CREATE TRIGGER IF NOT EXISTS change_log_model_versions_au
AFTER UPDATE ON model_versions
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'model_versions', new.space_id || ':' || CAST(new.version AS TEXT), 'update',
        json_object(
            'space_id', new.space_id, 'version', new.version,
            'weights_md5', new.weights_md5, 'card_md5', new.card_md5,
            'trained_at', new.trained_at
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

CREATE TRIGGER IF NOT EXISTS change_log_model_versions_ad
AFTER DELETE ON model_versions
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'model_versions', old.space_id || ':' || CAST(old.version AS TEXT), 'delete', NULL,
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
