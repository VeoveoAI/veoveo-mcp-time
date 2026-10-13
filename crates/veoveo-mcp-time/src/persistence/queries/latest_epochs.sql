-- The latest version of each requested epoch. The caller requests at most 100 keys.
SELECT DISTINCT ON (epoch_key)
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
WHERE tenant = $1 AND epoch_key = ANY($2)
ORDER BY epoch_key, epoch_version DESC
LIMIT 100
