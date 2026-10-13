-- Version numbers of one calendar whose decimal form starts with $3; the page index seeks
-- the calendar.
SELECT DISTINCT calendar_version::text AS "candidate!"
FROM time.calendar_version
WHERE tenant = $1 AND calendar_key = $2 AND starts_with(calendar_version::text, $3)
ORDER BY 1
LIMIT $4
