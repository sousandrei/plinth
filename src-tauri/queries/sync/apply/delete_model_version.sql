-- Delete a single `(space_id, version)` row from `model_versions`.
-- The change_log trigger emits `row_id` as `"space_id:version"` (see
-- `change_log_model_versions_ad` in `0003_model_versions.sql`), so we
-- split the composite here. Files matching that version are NOT deleted
-- here — the model-sync phase GC handles file cleanup so the apply path
-- doesn't need an `AppHandle`.
DELETE FROM model_versions
WHERE space_id = ?1 AND version = CAST(?2 AS INTEGER);
