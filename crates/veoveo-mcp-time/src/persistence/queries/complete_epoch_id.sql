-- Epoch IDs that start with a prefix: the keys in [$2, $3), a range the page index seeks.
SELECT DISTINCT epoch_key::text AS "candidate!"
FROM time.epoch
WHERE tenant = $1 AND epoch_key >= $2 AND epoch_key < $3
ORDER BY 1
LIMIT $4
