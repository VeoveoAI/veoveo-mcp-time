SELECT
    tenant AS "tenant: TenantId",
    owner AS "owner: PrincipalId",
    maximum_error_nanoseconds,
    maximum_stratum,
    minimum_source_diversity,
    maximum_holdover_seconds,
    record_version,
    created_at,
    updated_at
FROM time.clock_policy
WHERE tenant = $1
