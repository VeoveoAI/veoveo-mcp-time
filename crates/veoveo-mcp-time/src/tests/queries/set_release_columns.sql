UPDATE time.authority_release
SET
    tenant = $3,
    dataset_kind = $4,
    release_key = $5,
    state = $6,
    source_digest = $7,
    artifact_path = $8
WHERE tenant = $1 AND release_key = $2
RETURNING 1 AS "updated!"
