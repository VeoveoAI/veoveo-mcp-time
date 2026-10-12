SELECT
    tenant AS "tenant: TenantId",
    acquisition_key AS "acquisition_key: TimeAcquisitionId",
    owner AS "owner: PrincipalId",
    source_key AS "source_key: TimeSourceId",
    expected_source_digest,
    idempotency_key,
    status AS "status: TimeAcquisitionState",
    phase AS "phase: TimeAcquisitionPhase",
    staged_release_key AS "staged_release_key: AuthorityReleaseId",
    body,
    record_version,
    created_at,
    updated_at
FROM time.acquisition
WHERE tenant = $1 AND staged_release_key = $2
ORDER BY created_at, acquisition_key
LIMIT 1
