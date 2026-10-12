-- Activations of one tenant run one at a time. The transaction lock covers pointers that
-- do not exist yet, which row locks cannot.
SELECT true AS "locked!"
FROM pg_advisory_xact_lock(hashtext('time.active_authority'), hashtext($1::uuid::text))
