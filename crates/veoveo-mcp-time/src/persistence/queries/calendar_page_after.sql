-- The tenant's calendar versions after a cursor position. The order mixes directions, so
-- no row comparison expresses the position. It splits into two seeks of the page index:
-- the cursor's calendar below its version, then the calendars after it. Each branch
-- reads at most one page, and every column is NOT NULL.
(
    SELECT
        tenant AS "tenant!: TenantId",
        calendar_key AS "calendar_key!: CalendarId",
        calendar_version AS "calendar_version!",
        owner AS "owner!: PrincipalId",
        name AS "name!",
        zone_id AS "zone_id!",
        state AS "state!: TimeCalendarState",
        body AS "body!",
        created_at AS "created_at!",
        updated_at AS "updated_at!",
        tenant_key AS "tenant_key!",
        owner_key AS "owner_key!",
        work_context AS "work_context!"
    FROM time.calendar_version
    WHERE tenant = $1 AND calendar_key = $2 AND calendar_version < $3
    ORDER BY calendar_version DESC
    LIMIT $4
)
UNION ALL
(
    SELECT
        tenant,
        calendar_key,
        calendar_version,
        owner,
        name,
        zone_id,
        state,
        body,
        created_at,
        updated_at,
        tenant_key,
        owner_key,
        work_context
    FROM time.calendar_version
    WHERE tenant = $1 AND calendar_key > $2
    ORDER BY calendar_key, calendar_version DESC
    LIMIT $4
)
ORDER BY 2, 3 DESC
LIMIT $4
