-- The plan of the production statement of the same name in src/persistence/queries.
-- persistence/plans.rs checks that everything after this header is that statement.
EXPLAIN (FORMAT JSON)
-- An owner's event IDs that start with a prefix: the keys in [$3, $4), a range the
-- owner key index seeks.
SELECT DISTINCT event_key::text AS "candidate!"
FROM time.event
WHERE tenant = $1 AND owner = $2 AND event_key >= $3 AND event_key < $4
ORDER BY 1
LIMIT $5
