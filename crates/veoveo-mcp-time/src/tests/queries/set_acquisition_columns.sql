UPDATE time.acquisition
SET phase = $3, staged_release_key = $4, expected_source_digest = $5
WHERE tenant = $1 AND acquisition_key = $2
RETURNING 1 AS "updated!"
