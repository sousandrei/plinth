-- List every device_id that has at least one space grant. Used by the
-- scheduler to know which peers to dial.
SELECT DISTINCT device_id AS "device_id!: String"
FROM space_devices
