SELECT COALESCE(MIN(seq), 0) AS "min_seq!: i64"
FROM change_log
WHERE space_id = ?1
  AND device_id = ?2
