-- V001: initial schema (spaces-based multi-user)
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
