-- The plan of the production statement of the same name in src/persistence/queries.
-- persistence/plans.rs checks that everything after this header is that statement.
EXPLAIN (FORMAT JSON)
-- veoveo: all tenants (each process's due loop changes the passed events of every tenant, and the row trigger notifies each tenant's own subscribers)
-- Moves at most $1 scheduled events whose instant has passed to due, the earliest first,
-- so a backlog becomes several short statements. Replicas may run it concurrently: SKIP
-- LOCKED gives each a different batch, and the state predicate changes each event once.
-- The body follows the lifecycle columns.
UPDATE time.event AS event
SET
    state = 'due',
    body = jsonb_set(
        jsonb_set(event.body, '{state}', '"due"'),
        '{recordVersion}',
        to_jsonb(event.record_version + 1)
    ),
    record_version = event.record_version + 1,
    updated_at = now()
FROM (
    SELECT tenant, event_key
    FROM time.event
    WHERE state = 'scheduled' AND due <= now() AND record_version < 9223372036854775807
    ORDER BY due
    LIMIT $1
    FOR UPDATE SKIP LOCKED
) AS batch
WHERE event.tenant = batch.tenant AND event.event_key = batch.event_key
RETURNING event.tenant AS "tenant: TenantId", event.event_key AS "event_key: EventId"
