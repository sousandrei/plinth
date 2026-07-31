-- List every active or revoking device's certificate with the spaces
-- they have a grant in. Used by resolve_peer to build a PeerIdentity
-- from a TLS cert.
--
-- Step 30.4: a row in space_devices means the device has *some* grant
-- (not necessarily 'active'). The session layer filters per-grant
-- according to trust_mode. `revocation_only` grants are included so
-- the peer is recognised at TLS time, but the session treats them as
-- read-only per the per-space filter.
--
-- The cert that resolves a TLS connection comes from `devices`; the
-- grant trust_mode here is what's checked at the session layer.
SELECT
    sd.space_id   AS "space_id!: String",
    sd.device_id  AS "device_id!: String",
    sd.trust_mode AS "trust_mode!: String",
    d.cert_pem    AS "cert_pem!: String"
FROM space_devices sd
JOIN devices d ON d.device_id = sd.device_id
INNER JOIN spaces s ON s.id = sd.space_id
WHERE sd.trust_mode IN ('active', 'revoking', 'revocation_only')
