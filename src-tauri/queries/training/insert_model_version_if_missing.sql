-- Insert a model_versions row only if no row exists for
-- (space_id, version). Used by the one-time startup backfill so
-- orphan files on disk cannot overwrite a synchronized manifest or
-- resurrect a remotely deleted row. See data/PLAN.md Step 28.4.
INSERT INTO model_versions (space_id, version, weights_md5, card_md5, trained_at)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (space_id, version) DO NOTHING;
