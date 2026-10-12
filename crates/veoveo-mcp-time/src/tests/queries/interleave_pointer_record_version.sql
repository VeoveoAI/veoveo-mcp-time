CREATE OR REPLACE FUNCTION time.test_interleave() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE time.active_authority SET record_version = 3
    WHERE tenant = NEW.tenant AND dataset_kind = NEW.dataset_kind;
    RETURN NULL;
END
$$
