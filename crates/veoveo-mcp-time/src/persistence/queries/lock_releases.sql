SELECT release_key AS "release_key: AuthorityReleaseId"
FROM time.authority_release
WHERE tenant = $1 AND release_key = ANY($2)
ORDER BY release_key
FOR UPDATE
