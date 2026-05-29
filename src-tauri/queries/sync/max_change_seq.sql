SELECT COALESCE(MAX(seq), 0) AS "max_seq!: i64"
FROM change_log
WHERE space_id = ?1
  AND device_id = ?2
