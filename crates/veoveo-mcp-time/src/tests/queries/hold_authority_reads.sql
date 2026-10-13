-- Holds every reader of Time's active authority pointers until this transaction ends.
LOCK TABLE time.active_authority IN ACCESS EXCLUSIVE MODE
