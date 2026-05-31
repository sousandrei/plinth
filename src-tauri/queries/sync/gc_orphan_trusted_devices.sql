-- Delete trusted_devices rows whose space no longer exists.
-- Safe to run at any time — no side effects, no triggers.

DELETE FROM trusted_devices
WHERE space_id NOT IN (SELECT id FROM spaces)
