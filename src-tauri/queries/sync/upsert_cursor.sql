-- Advance our cursor for `(space_id, peer_device_id)` to `last_seq`.
-- Idempotent on conflict; never moves a cursor backward — applying a
-- batch out of order would otherwise lose progress permanently.
INSERT INTO sync_cursors (space_id, peer_device_id, last_seq)
VALUES (?1, ?2, ?3)
ON CONFLICT(space_id, peer_device_id) DO UPDATE SET
    last_seq = MAX(sync_cursors.last_seq, excluded.last_seq)
