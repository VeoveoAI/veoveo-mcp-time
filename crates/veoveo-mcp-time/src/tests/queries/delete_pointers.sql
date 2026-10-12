DELETE FROM time.active_authority
WHERE tenant = $1
RETURNING 1 AS "deleted!"
