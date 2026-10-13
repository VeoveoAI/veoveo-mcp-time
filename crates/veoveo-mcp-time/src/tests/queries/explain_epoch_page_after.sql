-- The plan of the production statement of the same name in src/persistence/queries.
-- persistence/plans.rs checks that everything after this header is that statement.
EXPLAIN (FORMAT JSON)
-- The tenant's epoch versions after a cursor position. The order mixes directions, so
-- no row comparison expresses the position; `epoch_key >= $2` lets the page index seek
-- to the cursor's epoch, and the disjunction drops its versions already shown.
SELECT
    tenant AS "tenant: TenantId",
    epoch_key AS "epoch_key: EpochId",
    epoch_version,
    owner AS "owner: PrincipalId",
    name,
    tai_seconds_since_1970,
    nanosecond,
    body,
    created_at,
    updated_at,
    tenant_key,
    owner_key,
    work_context
FROM time.epoch
WHERE tenant = $1
    AND epoch_key >= $2
    AND (epoch_key > $2 OR epoch_version < $3)
ORDER BY epoch_key, epoch_version DESC
LIMIT $4
