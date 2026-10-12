INSERT INTO time.source (
    tenant, source_key, owner, name, dataset_kind, source_url, expected_content_type,
    enabled, body, record_version, created_at, updated_at
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1, now(), now())
ON CONFLICT (tenant, source_key) DO NOTHING
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
