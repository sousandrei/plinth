-- Insert an incoming change_log row with its original origin identity.
-- Used by the remote-apply path before materializing the row body.
-- ON CONFLICT DO NOTHING on the unique origin key makes relay
-- idempotent: the same change arriving through two relay paths is
-- inserted only once. See data/PLAN.md Step 29.2.
INSERT INTO change_log
    (id, space_id, table_name, row_id, operation, payload, seq, device_id,
     origin_device_id, origin_seq)
VALUES
    (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
ON CONFLICT (space_id, origin_device_id, origin_seq)
    WHERE origin_device_id IS NOT NULL AND origin_seq IS NOT NULL
    DO NOTHING
