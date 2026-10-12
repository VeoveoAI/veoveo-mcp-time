INSERT INTO time.acquisition (
    tenant, acquisition_key, owner, source_key, expected_source_digest, idempotency_key,
    status, phase, staged_release_key, body, record_version, created_at, updated_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1, now(), now())
ON CONFLICT (tenant, owner, idempotency_key) DO NOTHING
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
