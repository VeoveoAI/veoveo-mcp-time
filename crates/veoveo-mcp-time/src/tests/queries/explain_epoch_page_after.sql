-- The plan of the production statement of the same name in src/persistence/queries.
-- persistence/plans.rs checks that everything after this header is that statement.
EXPLAIN (FORMAT JSON)
-- The tenant's epoch versions after a cursor position. The order mixes directions, so no
-- row comparison expresses the position. It splits into two seeks of the page index: the
-- cursor's epoch below its version, then the epochs after it. Each branch reads at most
-- one page, and every column is NOT NULL.
(
    SELECT
        tenant AS "tenant!: TenantId",
        epoch_key AS "epoch_key!: EpochId",
        epoch_version AS "epoch_version!",
        owner AS "owner!: PrincipalId",
        name AS "name!",
        tai_seconds_since_1970 AS "tai_seconds_since_1970!",
        nanosecond AS "nanosecond!",
        body AS "body!",
        created_at AS "created_at!",
        updated_at AS "updated_at!",
        tenant_key AS "tenant_key!",
        owner_key AS "owner_key!",
        work_context AS "work_context!"
    FROM time.epoch
    WHERE tenant = $1 AND epoch_key = $2 AND epoch_version < $3
    ORDER BY epoch_version DESC
    LIMIT $4
)
UNION ALL
(
    SELECT
        tenant,
        epoch_key,
        epoch_version,
        owner,
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
    WHERE tenant = $1 AND epoch_key > $2
    ORDER BY epoch_key, epoch_version DESC
    LIMIT $4
)
ORDER BY 2, 3 DESC
LIMIT $4
