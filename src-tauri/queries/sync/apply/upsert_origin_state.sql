-- Upsert per-origin high-water mark and retained floor during snapshot apply.
INSERT INTO origin_state (space_id, origin_device_id, high_water, retained_floor)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT (space_id, origin_device_id) DO UPDATE
SET high_water = MAX(excluded.high_water, origin_state.high_water),
    retained_floor = excluded.retained_floor
