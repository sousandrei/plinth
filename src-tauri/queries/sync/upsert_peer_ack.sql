-- Upsert a peer's acknowledgment for one origin. Called when the
-- recv half receives Frame::AppliedCursors — the entries represent
-- what the peer (consuming_device_id) has actually consumed, not
-- what we consumed. MAX ensures monotonic progress even if a peer
-- reports a lower seq due to a bug or reconnection edge case.
-- See data/PLAN.md Step 29.6.
INSERT INTO peer_acks (space_id, consuming_device_id, origin_device_id, last_applied_seq)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT (space_id, consuming_device_id, origin_device_id) DO UPDATE
SET last_applied_seq = MAX(excluded.last_applied_seq, peer_acks.last_applied_seq)
