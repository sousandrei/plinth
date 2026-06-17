SELECT id, name, color
FROM categories
WHERE space_id = ?1
ORDER BY name ASC
