-- The releases the tenant's pointers select, when each is active and of the pointer's
-- family. A pointer without a row here references a release that is no longer active.
SELECT
    r.tenant AS "tenant: TenantId",
    r.release_key AS "release_key: AuthorityReleaseId",
    r.owner AS "owner: PrincipalId",
    r.source_key AS "source_key: TimeSourceId",
    r.dataset_kind AS "dataset_kind: TimeDatasetKind",
    r.state AS "state: TimeAuthorityReleaseState",
    r.version_label,
    r.source_url,
    r.source_digest,
    r.artifact_path,
    r.retrieved_at,
    r.validated_at,
    r.body,
    r.record_version,
    r.created_at,
    r.updated_at,
    r.tenant_key,
    r.owner_key,
    r.work_context
FROM time.active_authority AS p
JOIN time.authority_release AS r
    ON r.tenant = p.tenant
    AND r.dataset_kind = p.dataset_kind
    AND r.release_key = p.release_key
    AND r.state = 'active'
WHERE p.tenant = $1
ORDER BY p.dataset_kind
