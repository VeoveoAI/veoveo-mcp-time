CREATE OR REPLACE FUNCTION time.test_interleave() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE time.active_authority SET release_key = 'time-release-changed'
    WHERE tenant = NEW.tenant AND dataset_kind = NEW.dataset_kind;
    RETURN NULL;
END
$$
