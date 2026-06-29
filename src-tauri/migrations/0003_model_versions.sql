-- Per-version metadata + MD5s for every trained model in every space.
-- The on-disk files (`model_v{N}.safetensors`, `model_v{N}.json`) are still
-- the source of truth for weights; this table is the mesh-wide registry of
-- which versions exist, authored by whom, and what their MD5s are. Peer
-- file-sync compares `model_versions.{weights,card}_md5` against the
-- actual on-disk bytes to detect divergence and trigger re-transfer.
--
-- Stamped `device_id` from the `applying_as_device` override (or the
-- local `device_id` setting) so writes applied on behalf of a remote
-- peer don't echo back. See `queries/sync/set_apply_override.sql`.
CREATE TABLE IF NOT EXISTS model_versions (
    space_id    TEXT    NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    weights_md5 TEXT    NOT NULL,
    card_md5    TEXT    NOT NULL,
    trained_at  TEXT    NOT NULL,
    PRIMARY KEY (space_id, version)
);

CREATE INDEX IF NOT EXISTS model_versions_space_idx
    ON model_versions (space_id);

CREATE TRIGGER IF NOT EXISTS change_log_model_versions_ai
AFTER INSERT ON model_versions BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_model_versions_au
AFTER UPDATE ON model_versions BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
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
        )
    );
END;

CREATE TRIGGER IF NOT EXISTS change_log_model_versions_ad
AFTER DELETE ON model_versions BEGIN
    UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
        WHERE key = 'sync_seq';
    INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
    VALUES (
        lower(hex(randomblob(16))),
        old.space_id, 'model_versions', old.space_id || ':' || CAST(old.version AS TEXT), 'delete', NULL,
        (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
        COALESCE(
            (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
            (SELECT value FROM app_settings WHERE key = 'device_id')
        )
    );
END;
