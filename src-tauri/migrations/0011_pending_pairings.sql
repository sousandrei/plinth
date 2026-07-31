-- V011: Durable host-side state for in-flight pairing commits.
--
-- Pending devices are intentionally separate from `space_devices`: they must
-- not enter the trusted peer set or change_log until the joiner acknowledges
-- that its snapshot transaction has committed.

CREATE TABLE IF NOT EXISTS pending_pairings (
    space_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    cert_pem TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (space_id, device_id)
);

CREATE INDEX IF NOT EXISTS idx_pending_pairings_created_at
    ON pending_pairings (created_at);
