-- List every space_settings row for a space. Used by the pairing flow
-- to ship the full snapshot of settings to a newly joining device,
-- so sync catch-up isn't needed for settings that may already have been
-- GC'd from the host's change_log.
SELECT space_id, key, value
FROM space_settings
WHERE space_id = ?1