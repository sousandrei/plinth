-- LWW upsert for `model_versions`. The `model_versions` table is the
-- mesh-wide registry of trained-model metadata + MD5s; the on-disk
-- weights and card files are transferred separately via `ModelData`
-- frames, so this upsert only touches the DB row. The model-sync GC
-- step cleans up orphan files whose `model_versions` row was deleted.
INSERT INTO model_versions (space_id, version, weights_md5, card_md5, trained_at)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (space_id, version) DO UPDATE SET
    weights_md5 = excluded.weights_md5,
    card_md5    = excluded.card_md5,
    trained_at  = excluded.trained_at;
