-- V012: Associate users with the installations they use within a space.
-- A space membership grants a person access to the space; this table records
-- which authorized users are represented by each installation.

CREATE TABLE IF NOT EXISTS device_user_grants (
    space_id  TEXT NOT NULL,
    device_id TEXT NOT NULL,
    user_id   TEXT NOT NULL,
    granted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (space_id, device_id, user_id),
    FOREIGN KEY (space_id, device_id)
        REFERENCES space_devices (space_id, device_id) ON DELETE CASCADE,
    FOREIGN KEY (space_id, user_id)
        REFERENCES space_members (space_id, user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_device_user_grants_user
    ON device_user_grants (space_id, user_id);

CREATE INDEX IF NOT EXISTS idx_device_user_grants_device
    ON device_user_grants (space_id, device_id);
