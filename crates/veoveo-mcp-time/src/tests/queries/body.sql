-- The stored body of one row, chosen by table name and key.
SELECT body AS "body!"
FROM (
    SELECT body FROM time.source
    WHERE $1::text = 'source' AND tenant = $2 AND source_key = $3
    UNION ALL
    SELECT body FROM time.authority_release
    WHERE $1 = 'authority_release' AND tenant = $2 AND release_key = $3
    UNION ALL
    SELECT body FROM time.acquisition
    WHERE $1 = 'acquisition' AND tenant = $2 AND acquisition_key = $3
    UNION ALL
    SELECT body FROM time.calendar_version
    WHERE $1 = 'calendar_version' AND tenant = $2 AND calendar_key = $3
    UNION ALL
    SELECT body FROM time.epoch
    WHERE $1 = 'epoch' AND tenant = $2 AND epoch_key = $3
    UNION ALL
    SELECT body FROM time.event
    WHERE $1 = 'event' AND tenant = $2 AND event_key = $3
) AS bodies
