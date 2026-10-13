-- veoveo: all tenants (each process's due loop sleeps until the earliest scheduled event of any tenant)
SELECT
    (SELECT min(due) FROM time.event WHERE state = 'scheduled') AS next_due,
    now() AS "now!"
