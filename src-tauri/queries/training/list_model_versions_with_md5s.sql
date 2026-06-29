-- List every `(weights_md5, card_md5)` row for one space as a single
-- query, used by `model_sync::local_summary` and the model-sync phase
-- file GC. Returns the canonical registry of what versions exist in
-- the mesh for this space; the on-disk bytes are checked separately.
SELECT version, weights_md5, card_md5
FROM model_versions
WHERE space_id = ?1
ORDER BY version ASC;
