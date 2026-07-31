-- Check if a space requires V2 reconciliation. Returns 1 if
-- reconciliation is required, 0 otherwise. See data/PLAN.md Step 29.8.
SELECT COALESCE(
    (SELECT required FROM v2_reconciliation WHERE space_id = ?1),
    0
) AS "required!: i64"
