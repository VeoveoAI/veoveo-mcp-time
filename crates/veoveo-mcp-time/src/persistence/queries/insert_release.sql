INSERT INTO time.authority_release (
    tenant, release_key, owner, source_key, dataset_kind, state, version_label, source_url,
    source_digest, artifact_path, retrieved_at, validated_at, body, record_version,
    created_at, updated_at, tenant_key, owner_key, work_context
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 1, now(), now(), $14, $15, $16)
ON CONFLICT (tenant, release_key) DO NOTHING
RETURNING
    tenant AS "tenant: TenantId",
    release_key AS "release_key: AuthorityReleaseId",
    owner AS "owner: PrincipalId",
    source_key AS "source_key: TimeSourceId",
    dataset_kind AS "dataset_kind: TimeDatasetKind",
    state AS "state: TimeAuthorityReleaseState",
    version_label,
    source_url,
    source_digest,
    artifact_path,
    retrieved_at,
    validated_at,
    body,
    record_version,
    created_at,
    updated_at,
    tenant_key,
    owner_key,
    work_context
