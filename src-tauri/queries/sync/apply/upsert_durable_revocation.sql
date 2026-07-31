INSERT INTO durable_revocations (
    revocation_id, space_id, target_device_id, certificate_fingerprint,
    winning_revision, requesting_owner_id, status, target_acknowledged, created_at
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
ON CONFLICT (revocation_id) DO UPDATE SET
    certificate_fingerprint = excluded.certificate_fingerprint,
    winning_revision = excluded.winning_revision,
    requesting_owner_id = excluded.requesting_owner_id,
    status = excluded.status,
    target_acknowledged = excluded.target_acknowledged,
    created_at = excluded.created_at
