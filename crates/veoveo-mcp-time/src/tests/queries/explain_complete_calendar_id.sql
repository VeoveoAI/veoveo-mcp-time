-- The plan of the production statement of the same name in src/persistence/queries.
-- persistence/plans.rs checks that everything after this header is that statement.
EXPLAIN (FORMAT JSON)
-- Calendar IDs that start with a prefix: the keys in [$2, $3), a range the page index seeks.
SELECT DISTINCT calendar_key::text AS "candidate!"
FROM time.calendar_version
WHERE tenant = $1 AND calendar_key >= $2 AND calendar_key < $3
ORDER BY 1
LIMIT $4
