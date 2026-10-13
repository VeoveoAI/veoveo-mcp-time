-- How many sessions wait for a lock on Time's active authority pointers.
SELECT count(*) AS "waiting!"
FROM pg_locks
WHERE locktype = 'relation'
    AND relation = 'time.active_authority'::regclass
    AND NOT granted
