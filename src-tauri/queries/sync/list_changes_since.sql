SELECT
    id          AS "id!",
    space_id    AS "space_id!",
    table_name  AS "table_name!",
    row_id      AS "row_id!",
    operation   AS "operation!",
    payload     AS "payload",
    seq         AS "seq!: i64",
    device_id   AS "device_id!",
    changed_at  AS "changed_at!"
FROM change_log
WHERE space_id = ?1
  AND device_id = ?2
  AND seq > ?3
ORDER BY seq ASC
LIMIT ?4
