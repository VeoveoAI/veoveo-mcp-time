CREATE OR REPLACE FUNCTION time.test_interleave() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE time.authority_release SET record_version = 3
    WHERE tenant = NEW.tenant AND dataset_kind = NEW.dataset_kind
        AND release_key <> NEW.release_key AND state = 'active';
    RETURN NULL;
END
$$
