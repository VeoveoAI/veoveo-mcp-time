DELETE FROM time.authority_release
WHERE tenant = $1 AND release_key = $2
RETURNING 1 AS "deleted!"
