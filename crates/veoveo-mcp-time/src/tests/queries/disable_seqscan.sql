-- Plan this transaction's statements without sequential scans, so each plan names the
-- index that serves it even on a nearly empty table.
SELECT set_config('enable_seqscan', 'off', true) AS "enable_seqscan!"
