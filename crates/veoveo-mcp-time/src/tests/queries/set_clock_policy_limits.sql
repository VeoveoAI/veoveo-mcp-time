UPDATE time.clock_policy
SET
    maximum_error_nanoseconds = $2,
    maximum_stratum = $3,
    minimum_source_diversity = $4,
    maximum_holdover_seconds = $5
WHERE tenant = $1
RETURNING 1 AS "updated!"
