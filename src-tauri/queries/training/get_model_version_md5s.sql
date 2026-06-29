-- Lookup one row from `model_versions` by composite `(space_id, version)`.
-- Used by the model-sync phase to verify that an incoming `ModelData`
-- frame's claimed MD5 matches the canonical mesh-wide MD5 (the LWW
-- resolution against any concurrent local INSERT/UPDATE). Returns
-- `NULL` when no row exists yet — the caller treats this as
-- "unauthorised; ignore the model files" (e.g., we received the
-- binary out of order, before the change_log INSERT arrived).
SELECT weights_md5, card_md5
FROM model_versions
WHERE space_id = ?1 AND version = ?2;
