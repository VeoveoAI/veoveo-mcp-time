SELECT DISTINCT epoch_key::text AS "candidate!"
FROM time.epoch
WHERE tenant = $1 AND strpos(lower(epoch_key), $2) > 0
ORDER BY 1
LIMIT $3
