-- Durable gap detection state for one origin. Returns origin_state
-- high_water/retained_floor (durable, survives GC) plus the live
-- max/min seq from change_log (may be 0 if the log was collected).
-- See data/PLAN.md Step 29.8.
SELECT
    COALESCE(os.high_water, 0)          AS "high_water!: i64",
    COALESCE(os.retained_floor, 0)      AS "retained_floor!: i64",
    COALESCE(cl_live.max_seq, 0)         AS "live_max_seq!: i64",
    COALESCE(cl_live.min_seq, 0)         AS "live_min_seq!: i64"
FROM (SELECT 1) AS dummy
LEFT JOIN origin_state os
    ON os.space_id = ?1 AND os.origin_device_id = ?2
LEFT JOIN (
    SELECT
        MAX(seq) AS max_seq,
        MIN(seq) AS min_seq
    FROM change_log
    WHERE space_id = ?1 AND device_id = ?2
) cl_live ON 1=1
