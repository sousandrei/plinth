UPDATE transactions
SET approved = COALESCE(?1, approved),
    note = COALESCE(?2, note),
    category = CASE WHEN ?3 THEN ?4 ELSE category END
WHERE id = ?5
  AND account_id IN (SELECT id FROM accounts WHERE space_id = ?6)
