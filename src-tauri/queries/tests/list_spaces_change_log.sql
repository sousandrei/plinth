-- Test assertion helper: list all change_log rows for the spaces
-- table in seq order. Returns (row_id, device_id) tuples so the test
-- can verify per-row attribution after a run_as_device call.
SELECT row_id, device_id
FROM change_log
WHERE table_name = 'spaces'
ORDER BY seq
