-- The first rows of a tenant's acquired releases in release ID order.
SELECT
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
FROM time.authority_release
WHERE tenant = $1
ORDER BY release_key
LIMIT $2
