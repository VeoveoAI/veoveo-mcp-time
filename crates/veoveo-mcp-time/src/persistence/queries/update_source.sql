UPDATE time.source
SET
    name = $3,
    dataset_kind = $4,
    source_url = $5,
    expected_content_type = $6,
    enabled = $7,
    body = $8,
    record_version = $10,
    updated_at = now()
WHERE tenant = $1 AND source_key = $2 AND record_version = $9
RETURNING
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
