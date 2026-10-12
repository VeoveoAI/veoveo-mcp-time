UPDATE time.active_authority
SET
    release_key = $3,
    previous_release_key = $4,
    activated_by = $5,
    activated_at = now(),
    record_version = $7
WHERE tenant = $1
    AND dataset_kind = $2
    AND release_key = $4
    AND previous_release_key IS NOT DISTINCT FROM $8
    AND record_version = $6
RETURNING record_version
