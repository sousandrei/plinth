-- Snapshot: list every `model_versions` row for this space. The
-- joiner receives the canonical MD5s up front, then the file bytes
-- over `Frame::ModelData` in the model-sync phase that follows the
-- snapshot stream. Ascending order keeps the wire stream deterministic.
SELECT space_id, CAST(version AS INTEGER) AS version, weights_md5, card_md5, trained_at
FROM model_versions
WHERE space_id = ?1
ORDER BY version ASC;
