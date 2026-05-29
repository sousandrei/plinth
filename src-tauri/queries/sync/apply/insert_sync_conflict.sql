-- Record a hot-field conflict surfaced by `conflict_detector` so the
-- UI can show a resolution panel. `resolved` defaults to 0 and
-- `created_at` to the current ISO 8601 timestamp via column defaults.
INSERT INTO sync_conflicts (
    id, space_id, table_name, row_id, field,
    local_value, remote_value, local_changed_at, remote_changed_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);
