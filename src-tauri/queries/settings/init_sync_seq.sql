INSERT INTO app_settings (key, value) VALUES ('sync_seq', '0')
ON CONFLICT (key) DO NOTHING
