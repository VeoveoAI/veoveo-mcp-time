UPDATE time.authority_release
SET state = 'retired', record_version = record_version + 1, updated_at = now()
WHERE tenant = $1
    AND dataset_kind = $2
    AND release_key = $3
    AND state = 'active'
    AND record_version = $4
    AND record_version < 9223372036854775807
RETURNING record_version
