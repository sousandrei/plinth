UPDATE transactions
SET approved = ?1, note = ?2, category = ?3
WHERE id = ?4
  AND account_id IN (SELECT id FROM accounts WHERE space_id = ?5)
