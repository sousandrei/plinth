-- Test assertion helper: returns the current `applying_as_device`
-- override value, or no row if the override is cleared. Used to
-- verify run_as_device cleans up on both success and failure paths.
SELECT value FROM app_settings WHERE key = 'applying_as_device'
