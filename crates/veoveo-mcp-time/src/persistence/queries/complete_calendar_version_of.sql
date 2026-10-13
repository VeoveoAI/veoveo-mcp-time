-- veoveo: scan (a decimal prefix of a version number is no range of the bigint index, so the filter reads the versions of the one calendar the index seeks)
-- Version numbers of one calendar whose decimal form starts with $3.
SELECT DISTINCT calendar_version::text AS "candidate!"
FROM time.calendar_version
WHERE tenant = $1 AND calendar_key = $2 AND starts_with(calendar_version::text, $3)
ORDER BY 1
LIMIT $4
