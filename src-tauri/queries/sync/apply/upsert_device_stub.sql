-- Ensure a stub devices row exists to satisfy the FK constraint from
-- space_devices. The full cert is synced separately via the pairing
-- flow. INSERT OR IGNORE makes this idempotent.
INSERT OR IGNORE INTO devices (device_id, cert_pem, display_name)
VALUES (?1, '', '')
