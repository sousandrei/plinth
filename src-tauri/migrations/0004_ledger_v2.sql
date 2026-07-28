-- V004: Replication Ledger V2 — add immutable origin identity, per-origin
-- high-water/floor state, per-row winner metadata, and real peer
-- acknowledgments. See data/PLAN.md Step 29.1.
--
-- This migration is additive: it creates new tables and columns without
-- touching existing triggers or application data. All existing spaces are
-- marked as requiring V2 reconciliation so the sync engine will route
-- them through snapshot reconciliation instead of trusting legacy cursor
-- state. Steps 29.2–29.8 will wire the new state into the apply path.

-- ---------------------------------------------------------------------------
-- 1. Immutable origin identity on change_log
-- ---------------------------------------------------------------------------
-- The existing (device_id, seq) columns are re-stamped by local triggers
-- during relay, so they cannot serve as an immutable origin key. Add
-- origin_device_id and origin_seq to carry the true origin identity.
-- Existing rows are bootstrapped best-effort: origin_device_id = device_id
-- (correct for both local and relayed rows), origin_seq = seq (exact for
-- local rows, wrong for relayed rows — but the space is marked for V2
-- reconciliation so it will be snapshotted fresh).

ALTER TABLE change_log ADD COLUMN origin_device_id TEXT;
ALTER TABLE change_log ADD COLUMN origin_seq INTEGER;

UPDATE change_log
SET origin_device_id = device_id,
    origin_seq = seq;

-- Unique immutable origin key. Partial index so new rows produced by the
-- pre-29.2 triggers (which leave origin columns NULL) do not conflict.
CREATE UNIQUE INDEX IF NOT EXISTS idx_change_log_origin
    ON change_log (space_id, origin_device_id, origin_seq)
    WHERE origin_device_id IS NOT NULL AND origin_seq IS NOT NULL;

-- ---------------------------------------------------------------------------
-- 2. Per-origin durable high-water and retained-floor state
-- ---------------------------------------------------------------------------
-- Survives change-log GC so gap detection works even after history is
-- collected. high_water is the max seq ever observed from this origin;
-- retained_floor is the min seq still present in change_log (0 if the
-- log has been fully collected for this origin).

CREATE TABLE IF NOT EXISTS origin_state (
    space_id          TEXT NOT NULL,
    origin_device_id  TEXT NOT NULL,
    high_water        INTEGER NOT NULL DEFAULT 0,
    retained_floor    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (space_id, origin_device_id)
);

INSERT INTO origin_state (space_id, origin_device_id, high_water, retained_floor)
SELECT space_id, device_id, MAX(seq), MIN(seq)
FROM change_log
GROUP BY space_id, device_id;

-- ---------------------------------------------------------------------------
-- 3. Per-row winner state
-- ---------------------------------------------------------------------------
-- Tracks the winning revision for each logical row so concurrent updates
-- can be adjudicated deterministically. deleted = 1 means the winning
-- change is a tombstone. Bootstrapped from the existing change_log: the
-- winner is the row with the highest seq (tiebreaker: device_id DESC).

CREATE TABLE IF NOT EXISTS row_winners (
    space_id        TEXT NOT NULL,
    table_name      TEXT NOT NULL,
    row_id          TEXT NOT NULL,
    winning_seq     INTEGER NOT NULL,
    winning_origin  TEXT NOT NULL,
    deleted         INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (space_id, table_name, row_id)
);

INSERT INTO row_winners (space_id, table_name, row_id, winning_seq, winning_origin, deleted)
WITH ranked AS (
    SELECT
        space_id, table_name, row_id, seq, device_id,
        CASE WHEN operation = 'delete' THEN 1 ELSE 0 END AS is_delete,
        ROW_NUMBER() OVER (
            PARTITION BY space_id, table_name, row_id
            ORDER BY seq DESC, device_id DESC
        ) AS rn
    FROM change_log
)
SELECT space_id, table_name, row_id, seq, device_id, is_delete
FROM ranked
WHERE rn = 1;

-- ---------------------------------------------------------------------------
-- 4. Peer acknowledgments
-- ---------------------------------------------------------------------------
-- What a peer has told us it consumed, keyed by (space, consuming device,
-- origin device). This is separate from sync_cursors (local receive
-- cursors) and replaces the incorrect GC inference that treated local
-- receive cursors as remote acknowledgments.

CREATE TABLE IF NOT EXISTS peer_acks (
    space_id            TEXT NOT NULL,
    consuming_device_id TEXT NOT NULL,
    origin_device_id    TEXT NOT NULL,
    last_applied_seq    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (space_id, consuming_device_id, origin_device_id)
);

CREATE INDEX IF NOT EXISTS idx_peer_acks_consuming
    ON peer_acks (space_id, consuming_device_id);

-- ---------------------------------------------------------------------------
-- 5. V2 reconciliation flag per space
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS v2_reconciliation (
    space_id TEXT PRIMARY KEY NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    required INTEGER NOT NULL DEFAULT 1
);

INSERT INTO v2_reconciliation (space_id, required)
SELECT id, 1 FROM spaces;
