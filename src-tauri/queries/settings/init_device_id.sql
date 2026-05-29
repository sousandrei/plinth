INSERT INTO app_settings (key, value) VALUES ('device_id', ?1)
ON CONFLICT (key) DO NOTHING
