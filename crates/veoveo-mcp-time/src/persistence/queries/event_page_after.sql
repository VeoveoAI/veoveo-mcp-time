-- An owner's events after a cursor position, optionally in one state. The row comparison
-- seeks the page index past the position; the state is a filter on the rows it reads.
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
WHERE tenant = $1
    AND owner = $2
    AND ($3::text IS NULL OR state = $3)
    AND (due_tai_seconds_since_1970, due_nanosecond, event_key) > ($4, $5, $6)
ORDER BY due_tai_seconds_since_1970, due_nanosecond, event_key
LIMIT $7
