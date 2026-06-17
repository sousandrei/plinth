UPDATE app_settings SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
WHERE key = 'sync_seq'