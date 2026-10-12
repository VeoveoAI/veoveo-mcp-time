UPDATE time.event
SET due = $3
WHERE tenant = $1 AND event_key = $2
RETURNING 1 AS "updated!"
