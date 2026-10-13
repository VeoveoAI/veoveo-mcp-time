-- Version numbers of any of the tenant's epochs whose decimal form starts with $2.
SELECT DISTINCT epoch_version::text AS "candidate!"
FROM time.epoch
WHERE tenant = $1 AND starts_with(epoch_version::text, $2)
ORDER BY 1
LIMIT $3
