-- V015: Durable certificate revocations (Phase 32.6).

CREATE TABLE IF NOT EXISTS durable_revocations (
    revocation_id          TEXT PRIMARY KEY NOT NULL,
    space_id               TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    target_device_id       TEXT NOT NULL REFERENCES devices(device_id),
    certificate_fingerprint TEXT NOT NULL,
    winning_revision       INTEGER NOT NULL,
    requesting_owner_id    TEXT NOT NULL REFERENCES users(id),
    status                 TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'revoked')),
    target_acknowledged    INTEGER NOT NULL DEFAULT 0,
    created_at             TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (space_id, target_device_id, winning_revision)
);

CREATE INDEX IF NOT EXISTS idx_durable_revocations_target
    ON durable_revocations (space_id, target_device_id, winning_revision DESC);

CREATE TRIGGER IF NOT EXISTS change_log_durable_revocations_ai
AFTER INSERT ON durable_revocations
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))), new.space_id, 'durable_revocations', new.revocation_id, 'insert',
        json_object(
            'revocation_id', new.revocation_id,
            'space_id', new.space_id,
            'target_device_id', new.target_device_id,
            'certificate_fingerprint', new.certificate_fingerprint,
            'winning_revision', new.winning_revision,
            'requesting_owner_id', new.requesting_owner_id,
            'status', new.status,
            'target_acknowledged', new.target_acknowledged,
            'created_at', new.created_at
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq')
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_durable_revocations_au
AFTER UPDATE ON durable_revocations
WHEN COALESCE((SELECT value FROM app_settings WHERE key = 'applying_remote'), '0') != '1'
BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id, origin_device_id, origin_seq)
    VALUES (
        lower(hex(randomblob(16))), new.space_id, 'durable_revocations', new.revocation_id, 'update',
        json_object(
            'revocation_id', new.revocation_id,
            'space_id', new.space_id,
            'target_device_id', new.target_device_id,
            'certificate_fingerprint', new.certificate_fingerprint,
            'winning_revision', new.winning_revision,
            'requesting_owner_id', new.requesting_owner_id,
            'status', new.status,
            'target_acknowledged', new.target_acknowledged,
            'created_at', new.created_at
        ),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        COALESCE((SELECT value FROM app_settings WHERE key = 'applying_as_device'), (SELECT value FROM app_settings WHERE key = 'device_id')),
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq')
    );
END;
