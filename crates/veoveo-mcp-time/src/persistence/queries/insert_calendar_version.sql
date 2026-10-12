INSERT INTO time.calendar_version (
    tenant, calendar_key, calendar_version, owner, name, zone_id, state, body, created_at,
    updated_at, tenant_key, owner_key, work_context
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), now(), $9, $10, $11)
ON CONFLICT (tenant, calendar_key, calendar_version) DO NOTHING
RETURNING
    tenant AS "tenant: TenantId",
    calendar_key AS "calendar_key: CalendarId",
    calendar_version,
    owner AS "owner: PrincipalId",
    name,
    zone_id,
    state AS "state: TimeCalendarState",
    body,
    created_at,
    updated_at,
    tenant_key,
    owner_key,
    work_context
