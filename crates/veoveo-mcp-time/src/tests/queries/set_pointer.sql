UPDATE time.active_authority
SET
    tenant = $3,
    dataset_kind = $4,
    release_key = $5,
    previous_release_key = $6,
    record_version = $7
WHERE tenant = $1 AND dataset_kind = $2
RETURNING 1 AS "updated!"
