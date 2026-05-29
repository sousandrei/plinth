-- Byte-identical today to `queries/spaces/remove_space_member.sql`,
-- intentionally duplicated. See `apply/upsert_account_summary.sql` for
-- the rationale.
DELETE FROM space_members WHERE space_id = ?1 AND user_id = ?2;
