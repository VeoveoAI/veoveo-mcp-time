-- Acquired release IDs that start with a prefix: the keys in [$2, $3), a range the
-- primary key seeks.
SELECT DISTINCT release_key::text AS "candidate!"
FROM time.authority_release
WHERE tenant = $1 AND release_key >= $2 AND release_key < $3
ORDER BY 1
LIMIT $4
