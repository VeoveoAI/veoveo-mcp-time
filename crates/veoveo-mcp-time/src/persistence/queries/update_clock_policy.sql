UPDATE time.clock_policy
SET
    owner = $2,
    maximum_error_nanoseconds = $3,
    maximum_stratum = $4,
    minimum_source_diversity = $5,
    maximum_holdover_seconds = $6,
    record_version = $8,
    updated_at = now()
WHERE tenant = $1 AND record_version = $7
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
