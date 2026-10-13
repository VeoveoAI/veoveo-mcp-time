-- Ends every other session of this login that holds an advisory lock, as a crash of its
-- process would. A Time replica's only such session holds its process instance's lock.
SELECT pg_terminate_backend(locks.pid) AS "terminated!"
FROM pg_locks AS locks
JOIN pg_stat_activity AS sessions ON sessions.pid = locks.pid
WHERE locks.locktype = 'advisory'
    AND locks.granted
    AND sessions.usename = current_user
    AND locks.pid <> pg_backend_pid()
