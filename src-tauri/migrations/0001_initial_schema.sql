-- V001: initial schema (spaces-based multi-user, with P2P sync state)
--
-- SQLite translation of the PostgreSQL schema with improvements:
--   - TIMESTAMPTZ  -> TEXT     (ISO 8601 "YYYY-MM-DD"; sorts lexicographically)
--   - BOOLEAN      -> INTEGER  (0 = false, 1 = true)
--   - email/picture dropped from users (local PIN auth replaces Google OAuth)
--   - pin_hash + created_at + updated_at added to users
--   - aggregated_months dropped — aggregations computed live from transactions
--   - pg_trgm full-text search replaced by FTS5 virtual table + sync triggers
--   - app_settings added for local preferences
--   - spaces + space_members + space_settings added for multi-user tenancy
--   - accounts.user_id replaced by accounts.space_id
--   - categories scoped per-space; default categories seeded at space creation
--   - P2P sync: trusted_devices, sync_cursors, change_log, sync_conflicts
--   - change_log triggers on every synced table (see data/PLAN.md)

CREATE TABLE IF NOT EXISTS users (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    pin_hash   TEXT,                           -- NULL until the user sets a PIN
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS spaces (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS space_members (
    space_id  TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    user_id   TEXT NOT NULL REFERENCES users(id)  ON DELETE CASCADE,
    role      TEXT NOT NULL DEFAULT 'member',   -- 'owner' | 'member'
    joined_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (space_id, user_id)
);

CREATE TABLE IF NOT EXISTS space_settings (
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    key      TEXT NOT NULL,
    value    TEXT NOT NULL,
    PRIMARY KEY (space_id, key)
);

CREATE TABLE IF NOT EXISTS categories (
    id       TEXT PRIMARY KEY NOT NULL,
    name     TEXT NOT NULL,
    color    TEXT NOT NULL DEFAULT '#6b7280',
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    UNIQUE (space_id, name)
);


CREATE TABLE IF NOT EXISTS accounts (
    id             TEXT PRIMARY KEY NOT NULL,
    name           TEXT NOT NULL,
    currency       TEXT NOT NULL,
    account_type   TEXT NOT NULL,
    account_source TEXT NOT NULL,
    color          TEXT NOT NULL DEFAULT '#6b7280',
    space_id       TEXT NOT NULL REFERENCES spaces (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS transactions (
    id           TEXT    PRIMARY KEY NOT NULL,
    booking_date TEXT    NOT NULL,              -- "YYYY-MM-DD"
    value_date   TEXT    NOT NULL,              -- "YYYY-MM-DD"
    reference    TEXT    NOT NULL DEFAULT '',
    text         TEXT    NOT NULL,
    currency     TEXT    NOT NULL,
    amount       INTEGER NOT NULL,              -- minor units (cents ×100)
    balance      INTEGER NOT NULL,
    approved     INTEGER NOT NULL DEFAULT 0,    -- 0 = false, 1 = true
    note         TEXT    NOT NULL DEFAULT '',
    category     TEXT,                          -- nullable; references categories.name within the space
    account_id   TEXT    NOT NULL REFERENCES accounts (id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS account_summaries (
    month      TEXT    NOT NULL,
    account_id TEXT    NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    balance    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (month, account_id)
);

CREATE TABLE IF NOT EXISTS app_settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

-- FTS5 virtual table — replaces pg_trgm for full-text search on transaction text
CREATE VIRTUAL TABLE IF NOT EXISTS transactions_fts USING fts5 (
    id   UNINDEXED,
    text,
    content       = 'transactions',
    content_rowid = 'rowid'
);

-- Keep FTS index in sync with the transactions table
CREATE TRIGGER IF NOT EXISTS transactions_ai AFTER INSERT ON transactions BEGIN
    INSERT INTO transactions_fts (rowid, id, text) VALUES (new.rowid, new.id, new.text);
END;

CREATE TRIGGER IF NOT EXISTS transactions_ad AFTER DELETE ON transactions BEGIN
    INSERT INTO transactions_fts (transactions_fts, rowid, id, text)
    VALUES ('delete', old.rowid, old.id, old.text);
END;

CREATE TRIGGER IF NOT EXISTS transactions_au AFTER UPDATE ON transactions BEGIN
    INSERT INTO transactions_fts (transactions_fts, rowid, id, text)
    VALUES ('delete', old.rowid, old.id, old.text);
    INSERT INTO transactions_fts (rowid, id, text) VALUES (new.rowid, new.id, new.text);
END;

-- Trigger to keep users.updated_at current
CREATE TRIGGER IF NOT EXISTS users_au AFTER UPDATE ON users BEGIN
    UPDATE users SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = new.id;
END;

-- Trigger to keep spaces.updated_at current
CREATE TRIGGER IF NOT EXISTS spaces_au AFTER UPDATE ON spaces BEGIN
    UPDATE spaces SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = new.id;
END;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_transactions_account_value_date
    ON transactions (account_id, value_date DESC, text ASC);

CREATE INDEX IF NOT EXISTS idx_transactions_approved
    ON transactions (account_id, approved);

CREATE INDEX IF NOT EXISTS idx_transactions_category
    ON transactions (account_id, category);

CREATE INDEX IF NOT EXISTS idx_transactions_value_date
    ON transactions (account_id, value_date);

CREATE INDEX IF NOT EXISTS idx_accounts_space_id
    ON accounts (space_id);

CREATE INDEX IF NOT EXISTS idx_categories_space_id
    ON categories (space_id);

CREATE INDEX IF NOT EXISTS idx_space_members_user_id
    ON space_members (user_id);

-- ---------------------------------------------------------------------------
-- P2P sync state
-- ---------------------------------------------------------------------------

-- Devices we have paired with, scoped per space. Removing a row here is
-- itself propagated through change_log so revocations reach every peer.
CREATE TABLE IF NOT EXISTS trusted_devices (
    id           TEXT PRIMARY KEY NOT NULL,
    space_id     TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    device_id    TEXT NOT NULL,
    display_name TEXT NOT NULL,
    cert_pem     TEXT NOT NULL,
    sync_enabled INTEGER NOT NULL DEFAULT 1,
    paired_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (space_id, device_id)
);

-- How far we've consumed each peer's change log, per space.
CREATE TABLE IF NOT EXISTS sync_cursors (
    space_id       TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    peer_device_id TEXT NOT NULL,
    last_seq       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (space_id, peer_device_id)
);

-- Every mutation to a synced table writes a row here via triggers below.
-- The application layer never writes here directly.
CREATE TABLE IF NOT EXISTS change_log (
    id          TEXT    PRIMARY KEY NOT NULL,
    space_id    TEXT    NOT NULL,
    table_name  TEXT    NOT NULL,
    row_id      TEXT    NOT NULL,
    operation   TEXT    NOT NULL,             -- 'insert' | 'update' | 'delete'
    payload     TEXT,                         -- JSON snapshot; NULL for delete
    seq         INTEGER NOT NULL,             -- per-device monotonic counter
    device_id   TEXT    NOT NULL,
    changed_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_change_log_space_seq
    ON change_log (space_id, device_id, seq);

CREATE INDEX IF NOT EXISTS idx_change_log_row
    ON change_log (space_id, table_name, row_id);

CREATE INDEX IF NOT EXISTS idx_change_log_changed_at
    ON change_log (changed_at);

-- Conflicts that LWW alone is too lossy for. Surfaced in the UI for the
-- user to resolve manually. Only category and note fields qualify.
CREATE TABLE IF NOT EXISTS sync_conflicts (
    id                TEXT PRIMARY KEY NOT NULL,
    space_id          TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    table_name        TEXT NOT NULL,
    row_id            TEXT NOT NULL,
    field             TEXT NOT NULL,         -- 'category' | 'note'
    local_value       TEXT,
    remote_value      TEXT,
    local_changed_at  TEXT NOT NULL,
    remote_changed_at TEXT NOT NULL,
    resolved          INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_sync_conflicts_unresolved
    ON sync_conflicts (space_id, resolved);

-- ---------------------------------------------------------------------------
-- change_log triggers
--
-- Every INSERT/UPDATE/DELETE on a synced table writes a row to change_log
-- with a fresh UUID, a snapshot of the row as JSON, and a monotonic seq
-- read from app_settings('sync_seq') and bumped atomically in the same
-- statement.
--
-- device_id resolution: the trigger stamps device_id from the override
-- key app_settings('applying_as_device') if set, else from
-- app_settings('device_id'). The override is used by the sync engine
-- when applying a peer's change_log row locally: the resulting log
-- entry then carries the originating peer's device_id, which keeps
-- (a) cursors meaningful for the original author and (b) replication
-- non-echoing — shipping queries filter on device_id = ours, so rows
-- stamped with a peer's id are never re-shipped.
--
-- The override must always be set/cleared inside the same SQLite
-- transaction as the data write — see sync::apply_guard.
--
-- Synced tables:
--   spaces, space_members, accounts, categories, transactions,
--   account_summaries, space_settings, trusted_devices
--
-- Not synced (intentionally):
--   users               -> propagated inline inside space_members payloads
--                          (so a new login can land on a peer device)
--   sync_cursors, change_log, sync_conflicts, app_settings
--   transactions_fts (derived)
--
-- The seq counter is stored as TEXT in app_settings and cast to INTEGER
-- inside the trigger. Initialized to '0' on first launch.
-- ---------------------------------------------------------------------------

-- spaces
CREATE TRIGGER IF NOT EXISTS change_log_spaces_ai
AFTER INSERT ON spaces BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_spaces_au
AFTER UPDATE ON spaces BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_spaces_ad
AFTER DELETE ON spaces BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        old.id, 'spaces', old.id, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

-- space_members
-- Payload embeds the full users row so a new login can materialize on a
-- peer device (the receiving side upserts the embedded user first, then
-- the membership row).
CREATE TRIGGER IF NOT EXISTS change_log_space_members_ai
AFTER INSERT ON space_members BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_space_members_au
AFTER UPDATE ON space_members BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_space_members_ad
AFTER DELETE ON space_members BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'space_members', old.space_id || ':' || old.user_id, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

-- users (no direct change_log entry — users aren't synced as a top-level
-- table). Instead, when a user is updated (e.g. PIN change, name edit),
-- emit a synthetic space_members update for every space they belong to.
-- The embedded users snapshot inside the space_members payload carries
-- the new values to peers.
CREATE TRIGGER IF NOT EXISTS change_log_users_au
AFTER UPDATE ON users BEGIN
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        -- one shared seq for the batch; bump once below
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq') + 1,
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    FROM space_members sm WHERE sm.user_id = new.id;

    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq'
          AND EXISTS (SELECT 1 FROM space_members WHERE user_id = new.id);
END;

-- accounts
CREATE TRIGGER IF NOT EXISTS change_log_accounts_ai
AFTER INSERT ON accounts BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_accounts_au
AFTER UPDATE ON accounts BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_accounts_ad
AFTER DELETE ON accounts BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'accounts', old.id, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

-- categories
CREATE TRIGGER IF NOT EXISTS change_log_categories_ai
AFTER INSERT ON categories BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'categories', new.id, 'insert',
        json_object('id', new.id, 'name', new.name, 'color', new.color, 'space_id', new.space_id),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_categories_au
AFTER UPDATE ON categories BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'categories', new.id, 'update',
        json_object('id', new.id, 'name', new.name, 'color', new.color, 'space_id', new.space_id),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_categories_ad
AFTER DELETE ON categories BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'categories', old.id, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

-- transactions
CREATE TRIGGER IF NOT EXISTS change_log_transactions_ai
AFTER INSERT ON transactions BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_transactions_au
AFTER UPDATE ON transactions BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_transactions_ad
AFTER DELETE ON transactions BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = old.account_id),
        'transactions', old.id, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

-- account_summaries
CREATE TRIGGER IF NOT EXISTS change_log_account_summaries_ai
AFTER INSERT ON account_summaries BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = new.account_id),
        'account_summaries', new.account_id || ':' || new.month, 'insert',
        json_object('month', new.month, 'account_id', new.account_id, 'balance', new.balance),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_account_summaries_au
AFTER UPDATE ON account_summaries BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = new.account_id),
        'account_summaries', new.account_id || ':' || new.month, 'update',
        json_object('month', new.month, 'account_id', new.account_id, 'balance', new.balance),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_account_summaries_ad
AFTER DELETE ON account_summaries BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        (SELECT space_id FROM accounts WHERE id = old.account_id),
        'account_summaries', old.account_id || ':' || old.month, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

-- space_settings
CREATE TRIGGER IF NOT EXISTS change_log_space_settings_ai
AFTER INSERT ON space_settings BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'space_settings', new.space_id || ':' || new.key, 'insert',
        json_object('space_id', new.space_id, 'key', new.key, 'value', new.value),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_space_settings_au
AFTER UPDATE ON space_settings BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        new.space_id, 'space_settings', new.space_id || ':' || new.key, 'update',
        json_object('space_id', new.space_id, 'key', new.key, 'value', new.value),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_space_settings_ad
AFTER DELETE ON space_settings BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'space_settings', old.space_id || ':' || old.key, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;

-- trusted_devices (revocations propagate through the log itself)
CREATE TRIGGER IF NOT EXISTS change_log_trusted_devices_ai
AFTER INSERT ON trusted_devices BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_trusted_devices_au
AFTER UPDATE ON trusted_devices BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_trusted_devices_ad
AFTER DELETE ON trusted_devices BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'trusted_devices', old.id, 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;
