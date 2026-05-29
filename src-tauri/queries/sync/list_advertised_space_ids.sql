-- All space IDs known on this device. Advertised over mDNS so peers can
-- decide whether to attempt a sync session with us.
--
-- In Step 3 this will narrow to spaces with at least one
-- trusted_devices row where sync_enabled = 1, so untrusted spaces are
-- never advertised.
SELECT id AS "id!"
FROM spaces
ORDER BY id
