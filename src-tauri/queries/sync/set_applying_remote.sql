INSERT INTO app_settings (key, value)
VALUES ('applying_remote', '1')
ON CONFLICT(key) DO UPDATE SET value = excluded.value
