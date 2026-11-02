UPDATE time.event
SET tenant_key = $3, owner_key = $3
WHERE tenant = $1 AND event_key = $2
RETURNING 1 AS "updated!"
