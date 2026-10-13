-- Version numbers of one epoch whose decimal form starts with $3; the page index seeks the
-- epoch.
SELECT DISTINCT epoch_version::text AS "candidate!"
FROM time.epoch
WHERE tenant = $1 AND epoch_key = $2 AND starts_with(epoch_version::text, $3)
ORDER BY 1
LIMIT $4
