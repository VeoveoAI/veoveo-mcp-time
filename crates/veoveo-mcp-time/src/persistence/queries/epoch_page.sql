SELECT
    tenant AS "tenant: TenantId",
    epoch_key AS "epoch_key: EpochId",
    epoch_version,
    owner AS "owner: PrincipalId",
    name,
    tai_seconds_since_1970,
    nanosecond,
    body,
    created_at,
    updated_at,
    tenant_key,
    owner_key,
    work_context
FROM time.epoch
WHERE tenant = $1
    AND (
        $2::text IS NULL
        OR epoch_key > $2
        OR (epoch_key = $2 AND epoch_version < $3)
    )
ORDER BY epoch_key, epoch_version DESC
LIMIT $4
