INSERT INTO change_log (id, space_id, table_name, row_id, operation, payload, seq, device_id)
VALUES (
    lower(hex(randomblob(16))),
    ?1, 'spaces', ?1, 'delete', NULL,
    (SELECT CAST(value AS INTEGER) FROM app_settings WHERE key = 'sync_seq'),
    COALESCE(
        (SELECT value FROM app_settings WHERE key = 'applying_as_device'),
        (SELECT value FROM app_settings WHERE key = 'device_id')
    )
)