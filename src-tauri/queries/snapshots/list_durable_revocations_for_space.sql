SELECT revocation_id, space_id, target_device_id, certificate_fingerprint,
       winning_revision, requesting_owner_id, status, target_acknowledged, created_at
FROM durable_revocations
WHERE space_id = ?1
ORDER BY winning_revision ASC, revocation_id ASC
