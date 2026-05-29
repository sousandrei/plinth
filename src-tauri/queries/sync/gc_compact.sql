-- 10.1 Compaction: for each (space_id, table_name, row_id) tuple, keep
-- only the highest-seq entry. Earlier inserts/updates are redundant once
-- a newer change exists for the same logical row. Runs after every
-- successful sync session. See data/PLAN.md §10.1.
DELETE FROM change_log
WHERE id NOT IN (
    SELECT id
    FROM change_log
    GROUP BY space_id, table_name, row_id
    HAVING seq = MAX(seq)
)
