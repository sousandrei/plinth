-- V001: initial schema
--
-- SQLite translation of the PostgreSQL schema with improvements:
--   - TIMESTAMPTZ  -> TEXT     (ISO 8601 "YYYY-MM-DD"; sorts lexicographically)
--   - BOOLEAN      -> INTEGER  (0 = false, 1 = true)
--   - email/picture dropped from users (local PIN auth replaces Google OAuth)
--   - pin_hash + created_at + updated_at added to users
--   - aggregated_months dropped — aggregations computed live from transactions
--   - pg_trgm full-text search replaced by FTS5 virtual table + sync triggers
--   - app_settings added for local preferences

CREATE TABLE IF NOT EXISTS users (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    pin_hash   TEXT,                           -- NULL until the user sets a PIN
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS categories (
    id    TEXT PRIMARY KEY NOT NULL,
    name  TEXT UNIQUE NOT NULL,
    color TEXT NOT NULL DEFAULT '#6b7280'
);

INSERT OR IGNORE INTO categories (id, name, color) VALUES
    ('cat_salary', 'Salary', '#22c55e'),
    ('cat_other_income', 'Other Income', '#10b981'),
    ('cat_savings_investments', 'Savings & Investments', '#06b6d4'),
    ('cat_household_services', 'Household & Services', '#3b82f6'),
    ('cat_transport', 'Transport', '#6366f1'),
    ('cat_food_drinks', 'Food & Drinks', '#f97316'),
    ('cat_shopping', 'Shopping', '#ec4899'),
    ('cat_health_beauty', 'Health & Beauty', '#8b5cf6'),
    ('cat_leisure', 'Leisure', '#eab308'),
    ('cat_other', 'Other', '#6b7280');

CREATE TABLE IF NOT EXISTS accounts (
    id             TEXT PRIMARY KEY NOT NULL,
    name           TEXT NOT NULL,
    currency       TEXT NOT NULL,
    account_type   TEXT NOT NULL,
    account_source TEXT NOT NULL,
    user_id        TEXT NOT NULL REFERENCES users (id)
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
    category     TEXT    REFERENCES categories (name), -- Nullable (can be uncategorized)
    account_id   TEXT    NOT NULL REFERENCES accounts (id)
);

CREATE TABLE IF NOT EXISTS account_summaries (
    month      TEXT    NOT NULL,
    account_id TEXT    NOT NULL REFERENCES accounts (id),
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

-- Indexes
CREATE INDEX IF NOT EXISTS idx_transactions_account_value_date
    ON transactions (account_id, value_date DESC, text ASC);

CREATE INDEX IF NOT EXISTS idx_transactions_approved
    ON transactions (account_id, approved);

CREATE INDEX IF NOT EXISTS idx_transactions_category
    ON transactions (account_id, category);

CREATE INDEX IF NOT EXISTS idx_transactions_value_date
    ON transactions (account_id, value_date);

CREATE INDEX IF NOT EXISTS idx_accounts_user_id
    ON accounts (user_id);
