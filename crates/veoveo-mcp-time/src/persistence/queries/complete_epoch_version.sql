SELECT DISTINCT epoch_version::text AS "candidate!"
FROM time.epoch
WHERE tenant = $1
    AND ($2::text IS NULL OR epoch_key = $2)
    AND strpos(epoch_version::text, $3) > 0
ORDER BY 1
LIMIT $4
