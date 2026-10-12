INSERT INTO time.event (
    tenant, event_key, owner, name, state, due_tai_seconds_since_1970, due_nanosecond, due,
    idempotency_key, body, record_version, created_at, updated_at, tenant_key, owner_key,
    work_context
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 1, now(), now(), $11, $12, $13)
ON CONFLICT (tenant, owner, idempotency_key) DO NOTHING
RETURNING
    tenant AS "tenant: TenantId",
    event_key AS "event_key: EventId",
    owner AS "owner: PrincipalId",
    name,
    state AS "state: TimeEventState",
    due_tai_seconds_since_1970,
    due_nanosecond,
    due,
    idempotency_key,
    body,
    record_version,
    created_at,
    updated_at,
    tenant_key,
    owner_key,
    work_context
