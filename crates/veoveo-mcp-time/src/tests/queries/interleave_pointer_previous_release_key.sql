CREATE OR REPLACE FUNCTION time.test_interleave() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE time.active_authority SET previous_release_key = NULL
    WHERE tenant = NEW.tenant AND dataset_kind = NEW.dataset_kind;
    RETURN NULL;
END
$$
