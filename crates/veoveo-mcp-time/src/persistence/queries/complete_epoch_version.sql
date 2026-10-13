-- veoveo: scan (a decimal prefix of a version number is no range of the bigint index, and without a selected epoch the filter reads every epoch version of the tenant)
-- Version numbers of any of the tenant's epochs whose decimal form starts with $2.
SELECT DISTINCT epoch_version::text AS "candidate!"
FROM time.epoch
WHERE tenant = $1 AND starts_with(epoch_version::text, $2)
ORDER BY 1
LIMIT $3
