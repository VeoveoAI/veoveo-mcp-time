-- Set the record version of one row, chosen by table name and key, bypassing the
-- catalog. The schema's checks still apply.
WITH
    s AS (
        UPDATE time.source SET record_version = $4
        WHERE $1::text = 'source' AND tenant = $2 AND source_key = $3 RETURNING 1
    ),
    r AS (
        UPDATE time.authority_release SET record_version = $4
        WHERE $1 = 'authority_release' AND tenant = $2 AND release_key = $3 RETURNING 1
    ),
    a AS (
        UPDATE time.acquisition SET record_version = $4
        WHERE $1 = 'acquisition' AND tenant = $2 AND acquisition_key = $3 RETURNING 1
    ),
    e AS (
        UPDATE time.event SET record_version = $4
        WHERE $1 = 'event' AND tenant = $2 AND event_key = $3 RETURNING 1
    ),
    p AS (
        UPDATE time.clock_policy SET record_version = $4
        WHERE $1 = 'clock_policy' AND tenant = $2 RETURNING 1
    )
SELECT (
    (SELECT count(*) FROM s) + (SELECT count(*) FROM r) + (SELECT count(*) FROM a)
    + (SELECT count(*) FROM e) + (SELECT count(*) FROM p)
) AS "updated!"
