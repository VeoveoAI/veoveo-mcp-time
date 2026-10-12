SELECT DISTINCT calendar_version::text AS "candidate!"
FROM time.calendar_version
WHERE tenant = $1
    AND ($2::text IS NULL OR calendar_key = $2)
    AND strpos(calendar_version::text, $3) > 0
ORDER BY 1
LIMIT $4
