-- Fetches the current category/note for a transaction, used as the
-- "local" side of hot-field conflict detection. Returns no rows if
-- the transaction hasn't been seen yet on this device.
SELECT category, note
FROM transactions
WHERE id = ?1;
