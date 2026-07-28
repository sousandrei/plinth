-- Update origin_state.retained_floor after GC collection.
-- retained_floor = MIN(origin_seq) of remaining rows, or 0 if empty.
-- high_water is never decreased (it was already set by gc_ensure_origin_state).
-- See data/PLAN.md Step 29.7.
UPDATE origin_state
SET retained_floor = COALESCE(
    (SELECT MIN(cl.origin_seq) FROM change_log cl
     WHERE cl.space_id = origin_state.space_id
       AND cl.origin_device_id = origin_state.origin_device_id),
    0
)
