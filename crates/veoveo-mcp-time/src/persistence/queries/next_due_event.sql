SELECT
    (SELECT min(due) FROM time.event WHERE state = 'scheduled') AS next_due,
    now() AS "now!"
