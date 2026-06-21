UPDATE transactions
SET approved = ?1
WHERE id IN (SELECT value FROM json_each(?2))
  AND account_id IN (SELECT id FROM accounts WHERE space_id = ?3)