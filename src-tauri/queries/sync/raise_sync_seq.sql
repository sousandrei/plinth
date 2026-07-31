-- Raise the local sequence clock to at least the incoming origin_seq.
-- This is the Lamport clock rule: after observing a remote change with
-- seq N, the next local write gets seq > N. See data/PLAN.md Step 29.3.
UPDATE app_settings
SET value = CAST(MAX(CAST(value AS INTEGER), ?1) AS TEXT)
WHERE key = 'sync_seq'
