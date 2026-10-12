SELECT
    tenant AS "tenant: TenantId",
    dataset_kind AS "dataset_kind: TimeDatasetKind",
    release_key AS "release_key: AuthorityReleaseId",
    previous_release_key AS "previous_release_key: AuthorityReleaseId",
    activated_by AS "activated_by: PrincipalId",
    activated_at,
    record_version
FROM time.active_authority
WHERE tenant = $1
ORDER BY dataset_kind
