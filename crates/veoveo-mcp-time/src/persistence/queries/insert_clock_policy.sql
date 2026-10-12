INSERT INTO time.clock_policy (
    tenant, owner, maximum_error_nanoseconds, maximum_stratum, minimum_source_diversity,
    maximum_holdover_seconds, record_version, created_at, updated_at
)
VALUES ($1, $2, $3, $4, $5, $6, 1, now(), now())
ON CONFLICT (tenant) DO NOTHING
RETURNING
    tenant AS "tenant: TenantId",
    owner AS "owner: PrincipalId",
    maximum_error_nanoseconds,
    maximum_stratum,
    minimum_source_diversity,
    maximum_holdover_seconds,
    record_version,
    created_at,
    updated_at
