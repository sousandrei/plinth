DELETE FROM pending_pairings
WHERE space_id = ?1
  AND device_id = ?2;
