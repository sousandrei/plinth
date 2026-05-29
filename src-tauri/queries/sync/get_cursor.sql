-- Returns the last_seq we have applied from `peer_device_id` for
-- the given space, or NULL if we have never synced from that peer
-- in this space. Callers translate NULL to 0 (= request everything
-- from seq 1).
SELECT last_seq
FROM sync_cursors
WHERE space_id = ?1 AND peer_device_id = ?2
