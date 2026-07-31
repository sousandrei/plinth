-- Snapshot: list per-origin high-water vector and retained floors for a space.
SELECT origin_device_id, high_water, retained_floor
FROM origin_state
WHERE space_id = ?1
ORDER BY origin_device_id ASC
