INSERT INTO app_settings (key, value)
VALUES ('applying_as_device', ?1)
ON CONFLICT(key) DO UPDATE SET value = excluded.value
