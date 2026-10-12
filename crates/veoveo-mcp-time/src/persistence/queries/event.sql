SELECT
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
FROM time.event
WHERE tenant = $1 AND event_key = $2 AND owner = $3
