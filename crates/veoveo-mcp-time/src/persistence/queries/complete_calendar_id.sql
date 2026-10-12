SELECT DISTINCT calendar_key::text AS "candidate!"
FROM time.calendar_version
WHERE tenant = $1 AND strpos(lower(calendar_key), $2) > 0
ORDER BY 1
LIMIT $3
