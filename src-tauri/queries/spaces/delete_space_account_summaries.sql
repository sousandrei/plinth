DELETE FROM account_summaries
WHERE account_id IN (SELECT id FROM accounts WHERE space_id = ?1)
