INSERT INTO time.active_authority (
    tenant, dataset_kind, release_key, previous_release_key, activated_by, activated_at,
    record_version
)
VALUES ($1, $2, $3, NULL, $4, now(), 1)
ON CONFLICT (tenant, dataset_kind) DO NOTHING
RETURNING record_version
