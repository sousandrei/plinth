-- Ensure origin_state entries exist for all origins currently in
-- change_log, before GC collection. This must run BEFORE the DELETE
-- so that origins whose rows are all collected still have an
-- origin_state entry to update retained_floor to 0.
-- high_water is MAX'd to ensure it never decreases.
INSERT INTO origin_state (space_id, origin_device_id, high_water, retained_floor)
SELECT
    space_id,
    origin_device_id,
    COALESCE(MAX(origin_seq), 0),
    COALESCE(MIN(origin_seq), 0)
FROM change_log
WHERE origin_device_id IS NOT NULL AND origin_seq IS NOT NULL
GROUP BY space_id, origin_device_id
ON CONFLICT (space_id, origin_device_id) DO UPDATE
SET high_water = MAX(excluded.high_water, origin_state.high_water)
