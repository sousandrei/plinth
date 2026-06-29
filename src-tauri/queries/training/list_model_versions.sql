-- Ascending version list for one space. Drives `list_models` and the
-- model-sync phase file GC; the on-disk `.safetensors` / `.json` files
-- are validated separately by reading the directory in Rust.
SELECT version
FROM model_versions
WHERE space_id = ?1
ORDER BY version ASC;
