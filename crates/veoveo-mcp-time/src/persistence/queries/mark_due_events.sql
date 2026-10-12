-- Replicas may run this concurrently: a row lock and the state predicate change each
-- scheduled event once. The body follows the lifecycle columns.
UPDATE time.event
SET
    state = 'due',
    body = jsonb_set(
        jsonb_set(body, '{state}', '"due"'),
        '{recordVersion}',
        to_jsonb(record_version + 1)
    ),
    record_version = record_version + 1,
    updated_at = now()
WHERE state = 'scheduled' AND due <= now() AND record_version < 9223372036854775807
RETURNING tenant AS "tenant: TenantId", event_key AS "event_key: EventId"
