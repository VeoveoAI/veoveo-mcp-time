SELECT DISTINCT release_key::text AS "candidate!"
FROM time.authority_release
WHERE tenant = $1 AND strpos(lower(release_key), $2) > 0
ORDER BY 1
LIMIT $3
