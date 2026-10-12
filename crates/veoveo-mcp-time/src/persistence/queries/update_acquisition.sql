UPDATE time.acquisition
SET
    status = $3,
    phase = $4,
    staged_release_key = $5,
    body = $6,
    record_version = $8,
    updated_at = now()
WHERE tenant = $1 AND acquisition_key = $2 AND record_version = $7
RETURNING
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
