-- Upsert one row into `model_versions`. Called by `fine_tune` after the
-- weights + card files are written, with MD5s computed from the on-disk
-- bytes. LWW (last-writer-wins) covers simultaneous training on
-- different peers; the model-sync phase then reconciles files based on
-- the canonical MD5 in the table.
INSERT INTO model_versions (space_id, version, weights_md5, card_md5, trained_at)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT (space_id, version) DO UPDATE SET
    weights_md5 = excluded.weights_md5,
    card_md5    = excluded.card_md5,
    trained_at  = excluded.trained_at;
