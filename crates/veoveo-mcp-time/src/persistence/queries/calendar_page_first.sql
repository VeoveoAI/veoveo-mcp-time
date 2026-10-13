-- The first rows of a tenant's calendar versions: key ascending, then the newest version.
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
WHERE tenant = $1
ORDER BY calendar_key, calendar_version DESC
LIMIT $2
