-- Version numbers of any of the tenant's calendars whose decimal form starts with $2.
SELECT DISTINCT calendar_version::text AS "candidate!"
FROM time.calendar_version
WHERE tenant = $1 AND starts_with(calendar_version::text, $2)
ORDER BY 1
LIMIT $3
