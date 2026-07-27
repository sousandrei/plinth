-- Distinct (space_id, origin device_id) pairs present in the local
-- change_log. Used by the multi-device test harness to decide which
-- origin streams to ship during a direct sync round.
SELECT DISTINCT space_id, device_id FROM change_log ORDER BY space_id, device_id;
