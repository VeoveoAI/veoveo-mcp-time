SELECT
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
FROM time.calendar_version
WHERE tenant = $1 AND calendar_key = $2 AND calendar_version = $3
