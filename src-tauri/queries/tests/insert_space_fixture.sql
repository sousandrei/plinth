-- Test fixture: insert a space with caller-supplied id, name, and
-- timestamps. Used by sync apply tests to seed pre-existing rows
-- whose change_log attribution we can then assert against.
INSERT INTO spaces (id, name, created_at, updated_at)
VALUES (?1, ?2, ?3, ?4)
