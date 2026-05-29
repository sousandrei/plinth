-- Ownership check: returns 1 if the given account belongs to the
-- given space, 0 otherwise. Used by space-scoped commands as a
-- gatekeeper before performing writes on an account or its summaries.
SELECT COUNT(*) FROM accounts WHERE id = ?1 AND space_id = ?2
