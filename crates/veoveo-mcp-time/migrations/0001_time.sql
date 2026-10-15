-- Time's tables. Every row belongs to one tenant, and `owner` names the principal that
-- wrote it. Tenant and principal IDs come from the identity schema without foreign keys.
-- `body` holds the public JSON document; the typed columns repeat the fields that SQL
-- filters, orders, joins or constrains, and the catalog checks that both agree.

-- A Time key: its prefix, then a lowercase hyphenated RFC 9562 version 7 UUID.
CREATE FUNCTION time.is_key(value text, prefix text) RETURNS boolean
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    RETURN value ~ ('^' || prefix || '[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$');

-- Nonempty printable text of at most `max` bytes.
CREATE FUNCTION time.is_text(value text, max integer) RETURNS boolean
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    RETURN octet_length(value) BETWEEN 1 AND max AND value !~ '[[:cntrl:]]';

CREATE TABLE time.source (
    tenant uuid NOT NULL,
    source_key text COLLATE "C" NOT NULL CHECK (time.is_key(source_key, 'time-source-')),
    owner uuid NOT NULL,
    name text NOT NULL CHECK (time.is_text(name, 256)),
    dataset_kind text NOT NULL CHECK (dataset_kind IN ('tzdb', 'leap_seconds')),
    source_url text NOT NULL CHECK (source_url LIKE 'https://%'),
    expected_content_type text NOT NULL CHECK (time.is_text(expected_content_type, 128)),
    enabled boolean NOT NULL,
    body jsonb NOT NULL CHECK (jsonb_typeof(body) = 'object'),
    record_version bigint NOT NULL CHECK (record_version > 0),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (tenant, source_key)
);

COMMENT ON TABLE time.source IS
    'Retention: for the life of the tenant. Replacement changes a source in place, and '
    'Time deletes none, because releases and acquisitions reference their source.';

CREATE TABLE time.authority_release (
    tenant uuid NOT NULL,
    release_key text COLLATE "C" NOT NULL CHECK (time.is_key(release_key, 'time-release-')),
    owner uuid NOT NULL,
    source_key text COLLATE "C" NOT NULL,
    dataset_kind text NOT NULL CHECK (dataset_kind IN ('tzdb', 'leap_seconds')),
    state text NOT NULL CHECK (state IN ('staged', 'active', 'retired', 'quarantined')),
    version_label text NOT NULL CHECK (time.is_text(version_label, 256)),
    source_url text NOT NULL CHECK (source_url LIKE 'https://%'),
    source_digest text NOT NULL CHECK (source_digest ~ '^sha256:[0-9a-f]{64}$'),
    artifact_path text NOT NULL CHECK (artifact_path LIKE '/%'),
    retrieved_at timestamptz NOT NULL,
    validated_at timestamptz NOT NULL CHECK (validated_at >= retrieved_at),
    body jsonb NOT NULL CHECK (jsonb_typeof(body) = 'object'),
    record_version bigint NOT NULL CHECK (record_version > 0),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    tenant_key text NOT NULL,
    owner_key text NOT NULL,
    work_context text NOT NULL,
    PRIMARY KEY (tenant, release_key),
    UNIQUE (tenant, dataset_kind, release_key),
    UNIQUE (tenant, source_key, source_digest),
    FOREIGN KEY (tenant, source_key) REFERENCES time.source (tenant, source_key)
);

COMMENT ON TABLE time.authority_release IS
    'Retention: for the life of the tenant. A retired release stays, because instants, '
    'epochs and results bind its ID. Time deletes none.';

CREATE TABLE time.acquisition (
    tenant uuid NOT NULL,
    acquisition_key text COLLATE "C" NOT NULL
        CHECK (time.is_key(acquisition_key, 'time-acquisition-')),
    owner uuid NOT NULL,
    source_key text COLLATE "C" NOT NULL,
    expected_source_digest text CHECK (expected_source_digest ~ '^sha256:[0-9a-f]{64}$'),
    idempotency_key text NOT NULL CHECK (time.is_text(idempotency_key, 256)),
    status text NOT NULL
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancel_requested', 'cancelled')),
    phase text NOT NULL
        CHECK (phase IN ('queued', 'downloading', 'validating', 'complete', 'cancelling', 'cancelled', 'failed')),
    staged_release_key text COLLATE "C",
    body jsonb NOT NULL CHECK (jsonb_typeof(body) = 'object'),
    record_version bigint NOT NULL CHECK (record_version > 0),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (tenant, acquisition_key),
    UNIQUE (tenant, owner, idempotency_key),
    FOREIGN KEY (tenant, source_key) REFERENCES time.source (tenant, source_key),
    FOREIGN KEY (tenant, staged_release_key) REFERENCES time.authority_release (tenant, release_key)
);

COMMENT ON TABLE time.acquisition IS
    'Retention: for the life of the tenant. A finished acquisition stays as the provenance '
    'of the release it staged. Time deletes none.';


CREATE INDEX acquisition_source ON time.acquisition (tenant, source_key);

CREATE INDEX acquisition_staged_release ON time.acquisition (tenant, staged_release_key);

-- One active release per family and tenant. The pointer's history agrees with its
-- version: the first activation has no previous release, and every later one names it.
CREATE TABLE time.active_authority (
    tenant uuid NOT NULL,
    dataset_kind text NOT NULL CHECK (dataset_kind IN ('tzdb', 'leap_seconds')),
    release_key text COLLATE "C" NOT NULL,
    previous_release_key text COLLATE "C",
    activated_by uuid NOT NULL,
    activated_at timestamptz NOT NULL,
    record_version bigint NOT NULL CHECK (record_version > 0),
    PRIMARY KEY (tenant, dataset_kind),
    CHECK (previous_release_key <> release_key),
    CHECK ((record_version = 1) = (previous_release_key IS NULL)),
    FOREIGN KEY (tenant, dataset_kind, release_key)
        REFERENCES time.authority_release (tenant, dataset_kind, release_key),
    FOREIGN KEY (tenant, dataset_kind, previous_release_key)
        REFERENCES time.authority_release (tenant, dataset_kind, release_key)
);

COMMENT ON TABLE time.active_authority IS
    'Retention: one row per tenant and family for the life of the tenant. Activation '
    'replaces the row in place.';


CREATE INDEX active_authority_release
    ON time.active_authority (tenant, dataset_kind, release_key);
CREATE INDEX active_authority_previous_release
    ON time.active_authority (tenant, dataset_kind, previous_release_key);

CREATE TABLE time.calendar_version (
    tenant uuid NOT NULL,
    calendar_key text COLLATE "C" NOT NULL CHECK (time.is_key(calendar_key, 'calendar-')),
    calendar_version bigint NOT NULL CHECK (calendar_version > 0),
    owner uuid NOT NULL,
    name text NOT NULL CHECK (time.is_text(name, 256)),
    zone_id text NOT NULL CHECK (time.is_text(zone_id, 128)),
    state text NOT NULL CHECK (state IN ('staged', 'active', 'retired')),
    body jsonb NOT NULL CHECK (jsonb_typeof(body) = 'object'),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    tenant_key text NOT NULL,
    owner_key text NOT NULL,
    work_context text NOT NULL,
    PRIMARY KEY (tenant, calendar_key, calendar_version)
);

COMMENT ON TABLE time.calendar_version IS
    'Retention: for the life of the tenant. Calendar versions are immutable, and Time '
    'deletes none.';

CREATE INDEX calendar_version_page ON time.calendar_version
    (tenant, calendar_key, calendar_version DESC);

CREATE TABLE time.epoch (
    tenant uuid NOT NULL,
    epoch_key text COLLATE "C" NOT NULL CHECK (time.is_key(epoch_key, 'epoch-')),
    epoch_version bigint NOT NULL CHECK (epoch_version > 0),
    owner uuid NOT NULL,
    name text NOT NULL CHECK (time.is_text(name, 256)),
    tai_seconds_since_1970 bigint NOT NULL,
    nanosecond integer NOT NULL CHECK (nanosecond BETWEEN 0 AND 999999999),
    body jsonb NOT NULL CHECK (jsonb_typeof(body) = 'object'),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    tenant_key text NOT NULL,
    owner_key text NOT NULL,
    work_context text NOT NULL,
    PRIMARY KEY (tenant, epoch_key, epoch_version)
);

COMMENT ON TABLE time.epoch IS
    'Retention: for the life of the tenant. Epoch versions are immutable, and Time deletes '
    'none.';

CREATE INDEX epoch_page ON time.epoch (tenant, epoch_key, epoch_version DESC);

-- `due` is the UTC instant of the TAI coordinate, rounded up to the microsecond, so an
-- event never becomes due before its instant. The due loop reads and updates it.
CREATE TABLE time.event (
    tenant uuid NOT NULL,
    event_key text COLLATE "C" NOT NULL CHECK (time.is_key(event_key, 'event-')),
    owner uuid NOT NULL,
    name text NOT NULL CHECK (time.is_text(name, 256)),
    state text NOT NULL CHECK (state IN ('scheduled', 'due', 'cancelled')),
    due_tai_seconds_since_1970 bigint NOT NULL,
    due_nanosecond integer NOT NULL CHECK (due_nanosecond BETWEEN 0 AND 999999999),
    due timestamptz NOT NULL,
    idempotency_key text NOT NULL CHECK (time.is_text(idempotency_key, 256)),
    body jsonb NOT NULL CHECK (jsonb_typeof(body) = 'object'),
    record_version bigint NOT NULL CHECK (record_version > 0),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    tenant_key text NOT NULL,
    owner_key text NOT NULL,
    work_context text NOT NULL,
    PRIMARY KEY (tenant, event_key),
    UNIQUE (tenant, owner, idempotency_key)
);

COMMENT ON TABLE time.event IS
    'Retention: for the life of the tenant. Due and cancelled events stay for operator '
    'retirement. Time deletes none.';


-- Event ID completion reads one owner's keys from a prefix range, which this index seeks.
CREATE INDEX event_owner_key ON time.event (tenant, owner, event_key);

CREATE INDEX event_page ON time.event
    (tenant, owner, due_tai_seconds_since_1970, due_nanosecond, event_key);
CREATE INDEX event_scheduled_due ON time.event (due) WHERE state = 'scheduled';

CREATE TABLE time.clock_policy (
    tenant uuid PRIMARY KEY,
    owner uuid NOT NULL,
    maximum_error_nanoseconds bigint NOT NULL CHECK (maximum_error_nanoseconds > 0),
    maximum_stratum smallint NOT NULL CHECK (maximum_stratum BETWEEN 1 AND 15),
    minimum_source_diversity bigint NOT NULL
        CHECK (minimum_source_diversity BETWEEN 1 AND 4294967295),
    maximum_holdover_seconds bigint NOT NULL CHECK (maximum_holdover_seconds > 0),
    record_version bigint NOT NULL CHECK (record_version > 0),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

COMMENT ON TABLE time.clock_policy IS
    'Retention: one row per tenant for the life of the tenant. Replacement updates the row '
    'in place.';

-- Every change wakes listeners on the `time` channel. The kernel trigger publishes the
-- table, the operation, the key column named by its first argument and the tenant
-- column named by its second. Listeners reread current rows; a payload is never data.
CREATE TRIGGER notify_change AFTER INSERT OR UPDATE OR DELETE ON time.source
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('source_key', 'tenant');
CREATE TRIGGER notify_change AFTER INSERT OR UPDATE OR DELETE ON time.authority_release
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('release_key', 'tenant');
CREATE TRIGGER notify_change AFTER INSERT OR UPDATE OR DELETE ON time.acquisition
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('acquisition_key', 'tenant');
CREATE TRIGGER notify_change AFTER INSERT OR UPDATE OR DELETE ON time.active_authority
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('dataset_kind', 'tenant');
CREATE TRIGGER notify_change AFTER INSERT OR UPDATE OR DELETE ON time.calendar_version
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('calendar_key', 'tenant');
CREATE TRIGGER notify_change AFTER INSERT OR UPDATE OR DELETE ON time.epoch
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('epoch_key', 'tenant');
CREATE TRIGGER notify_change AFTER INSERT OR UPDATE OR DELETE ON time.event
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('event_key', 'tenant');
CREATE TRIGGER notify_change AFTER INSERT OR UPDATE OR DELETE ON time.clock_policy
    FOR EACH ROW EXECUTE FUNCTION kernel.notify_change('tenant', 'tenant');
