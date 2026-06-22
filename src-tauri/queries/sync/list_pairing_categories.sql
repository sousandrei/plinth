-- Pairing snapshot: list every category in a space with its space_id.
-- The export_categories.sql query doesn't include space_id (it's implicit
-- in the export JSON), so this is a pairing-specific variant that
-- carries space_id for the joiner to materialise the row.
SELECT id, name, color, space_id
FROM categories
WHERE space_id = ?1
ORDER BY name ASC