-- Delete one row from `model_versions`. The on-disk `.safetensors` /
-- `.json` files are intentionally not touched here — `delete_model`
-- removes them locally, and peers handle orphan-file cleanup in the
-- model-sync phase GC. Change_log propagates this DELETE so other
-- peers remove the row, then GC sweeps the orphan files.
DELETE FROM model_versions
WHERE space_id = ?1 AND version = ?2;
