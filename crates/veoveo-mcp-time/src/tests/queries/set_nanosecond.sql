-- Set the stored subsecond column of an epoch or an event, bypassing the catalog.
WITH
    p AS (
        UPDATE time.epoch SET nanosecond = $4
        WHERE $1::text = 'epoch' AND tenant = $2 AND epoch_key = $3 RETURNING 1
    ),
    e AS (
        UPDATE time.event SET due_nanosecond = $4
        WHERE $1 = 'event' AND tenant = $2 AND event_key = $3 RETURNING 1
    )
SELECT ((SELECT count(*) FROM p) + (SELECT count(*) FROM e)) AS "updated!"
