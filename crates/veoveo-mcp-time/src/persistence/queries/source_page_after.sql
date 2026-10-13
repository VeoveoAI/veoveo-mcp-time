-- The tenant's sources after a cursor position, seeking the primary key past it.
SELECT
    tenant AS "tenant: TenantId",
    source_key AS "source_key: TimeSourceId",
    owner AS "owner: PrincipalId",
    name,
    dataset_kind AS "dataset_kind: TimeDatasetKind",
    source_url,
    expected_content_type,
    enabled,
    body,
    record_version,
    created_at,
    updated_at
FROM time.source
WHERE tenant = $1 AND source_key > $2
ORDER BY source_key
LIMIT $3
