-- Fail a blocked statement of this transaction after the given interval.
SELECT set_config('lock_timeout', $1, true) AS "lock_timeout!"
