INSERT INTO durable_revocations (
    revocation_id, space_id, target_device_id, certificate_fingerprint,
    winning_revision, requesting_owner_id, status, target_acknowledged
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', 0)
