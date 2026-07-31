-- Step 30.4: list space IDs that have at least one `trust_mode = 'active'`
-- grant. Used by mDNS to populate the `spaces` TXT record. Peers without
-- any active grant for a space aren't invited to participate in normal
-- sync for it.
SELECT DISTINCT sd.space_id AS "space_id!: String"
FROM space_devices sd
WHERE sd.trust_mode = 'active'
