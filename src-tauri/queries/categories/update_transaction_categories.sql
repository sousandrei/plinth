UPDATE transactions
SET category = ?2
WHERE category = ?1
  AND account_id IN (
    SELECT id FROM accounts WHERE space_id = ?3
  )
