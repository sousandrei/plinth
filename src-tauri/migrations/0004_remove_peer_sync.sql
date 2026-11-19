DROP TRIGGER change_log_spaces_ai;
DROP TRIGGER change_log_spaces_au;
DROP TRIGGER change_log_spaces_ad;
DROP TRIGGER change_log_space_members_ai;
DROP TRIGGER change_log_space_members_au;
DROP TRIGGER change_log_space_members_ad;
DROP TRIGGER change_log_users_au;
DROP TRIGGER change_log_accounts_ai;
DROP TRIGGER change_log_accounts_au;
DROP TRIGGER change_log_accounts_ad;
DROP TRIGGER change_log_categories_ai;
DROP TRIGGER change_log_categories_au;
DROP TRIGGER change_log_categories_ad;
DROP TRIGGER change_log_transactions_ai;
DROP TRIGGER change_log_transactions_au;
DROP TRIGGER change_log_transactions_ad;
DROP TRIGGER change_log_account_summaries_ai;
DROP TRIGGER change_log_account_summaries_au;
DROP TRIGGER change_log_account_summaries_ad;
DROP TRIGGER change_log_space_settings_ai;
DROP TRIGGER change_log_space_settings_au;
DROP TRIGGER change_log_space_settings_ad;
DROP TRIGGER change_log_trusted_devices_ai;
DROP TRIGGER change_log_trusted_devices_au;
DROP TRIGGER change_log_trusted_devices_ad;
DROP TRIGGER change_log_model_versions_ai;
DROP TRIGGER change_log_model_versions_au;
DROP TRIGGER change_log_model_versions_ad;

DROP TABLE trusted_devices;
DROP TABLE evicted_devices;
DROP TABLE sync_cursors;
DROP TABLE change_log;
DROP TABLE sync_conflicts;
DROP TABLE model_versions;

DELETE FROM app_settings
WHERE key IN ('device_id', 'device_cert_pem', 'device_key_pem', 'sync_seq', 'applying_as_device');

DELETE FROM spaces WHERE deleted = 1;
ALTER TABLE spaces DROP COLUMN deleted;
