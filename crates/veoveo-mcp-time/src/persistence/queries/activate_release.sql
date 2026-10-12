UPDATE time.authority_release
SET state = 'active', body = $5, record_version = $6, updated_at = now()
WHERE tenant = $1
    AND release_key = $2
    AND dataset_kind = $3
    AND state = 'staged'
    AND record_version = $4
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
