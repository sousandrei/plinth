-- Delete space_devices rows whose space no longer exists.
DELETE FROM space_devices
WHERE space_id NOT IN (SELECT id FROM spaces)
