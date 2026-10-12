SELECT DISTINCT event_key::text AS "candidate!"
FROM time.event
WHERE tenant = $1 AND owner = $2 AND strpos(lower(event_key), $3) > 0
ORDER BY 1
LIMIT $4
