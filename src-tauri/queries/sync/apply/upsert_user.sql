-- Apply a `users` upsert ridden inside a `space_members` payload.
-- Users never have their own change_log entries; their rows propagate
-- via the embedded UserSnapshot inside SpaceMember payloads
-- (`data/PLAN.md §4`). Triggers fire for the wrapping space_members
-- write, not for this row.
INSERT INTO users (id, name, pin_hash, created_at, updated_at)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(id) DO UPDATE SET
    name       = excluded.name,
    pin_hash   = excluded.pin_hash,
    updated_at = excluded.updated_at;
