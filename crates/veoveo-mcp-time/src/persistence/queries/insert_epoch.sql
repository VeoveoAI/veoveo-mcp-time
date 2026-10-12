INSERT INTO time.epoch (
    tenant, epoch_key, epoch_version, owner, name, tai_seconds_since_1970, nanosecond, body,
    created_at, updated_at, tenant_key, owner_key, work_context
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now(), $9, $10, $11)
ON CONFLICT (tenant, epoch_key, epoch_version) DO NOTHING
RETURNING
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
