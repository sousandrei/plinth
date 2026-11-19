SELECT EXISTS (
    SELECT 1 FROM spaces WHERE id = ?1
    UNION ALL
    SELECT 1 FROM json_each(?2, '$.categories') AS entry
    JOIN categories ON categories.id = entry.value ->> 'id'
    UNION ALL
    SELECT 1 FROM json_each(?2, '$.accounts') AS entry
    JOIN accounts ON accounts.id = entry.value ->> 'id'
    UNION ALL
    SELECT 1 FROM json_each(?2, '$.transactions') AS entry
    JOIN transactions ON transactions.id = entry.value ->> 'id'
) AS "exists!: bool"
