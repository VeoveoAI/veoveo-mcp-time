-- Replace the body of one row, chosen by table name and key, bypassing the catalog.
WITH
    s AS (
        UPDATE time.source SET body = $4
        WHERE $1::text = 'source' AND tenant = $2 AND source_key = $3 RETURNING 1
    ),
    r AS (
        UPDATE time.authority_release SET body = $4
        WHERE $1 = 'authority_release' AND tenant = $2 AND release_key = $3 RETURNING 1
    ),
    a AS (
        UPDATE time.acquisition SET body = $4
        WHERE $1 = 'acquisition' AND tenant = $2 AND acquisition_key = $3 RETURNING 1
    ),
    c AS (
        UPDATE time.calendar_version SET body = $4
        WHERE $1 = 'calendar_version' AND tenant = $2 AND calendar_key = $3 RETURNING 1
    ),
    p AS (
        UPDATE time.epoch SET body = $4
        WHERE $1 = 'epoch' AND tenant = $2 AND epoch_key = $3 RETURNING 1
    ),
    e AS (
        UPDATE time.event SET body = $4
        WHERE $1 = 'event' AND tenant = $2 AND event_key = $3 RETURNING 1
    )
SELECT (
    (SELECT count(*) FROM s) + (SELECT count(*) FROM r) + (SELECT count(*) FROM a)
    + (SELECT count(*) FROM c) + (SELECT count(*) FROM p) + (SELECT count(*) FROM e)
) AS "updated!"
