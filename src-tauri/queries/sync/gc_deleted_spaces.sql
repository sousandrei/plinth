DELETE FROM spaces
WHERE deleted = 1
  AND NOT EXISTS (
    SELECT 1 FROM change_log WHERE change_log.space_id = spaces.id
  )