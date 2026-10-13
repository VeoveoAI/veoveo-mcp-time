-- veoveo: scan (a decimal prefix of a version number is no range of the bigint index, so the filter reads the versions of the one epoch the index seeks)
-- Version numbers of one epoch whose decimal form starts with $3.
SELECT DISTINCT epoch_version::text AS "candidate!"
FROM time.epoch
WHERE tenant = $1 AND epoch_key = $2 AND starts_with(epoch_version::text, $3)
ORDER BY 1
LIMIT $4
