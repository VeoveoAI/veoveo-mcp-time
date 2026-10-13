-- Every table states its retention, and every foreign key has an index whose leading
-- columns are the key's columns, as the server schema rules of veoveo-core require.

CREATE INDEX acquisition_source ON time.acquisition (tenant, source_key);
CREATE INDEX active_authority_release
    ON time.active_authority (tenant, dataset_kind, release_key);
CREATE INDEX active_authority_previous_release
    ON time.active_authority (tenant, dataset_kind, previous_release_key);

COMMENT ON TABLE time.source IS
    'Retention: for the life of the tenant. Replacement changes a source in place, and '
    'Time deletes none, because releases and acquisitions reference their source.';
COMMENT ON TABLE time.authority_release IS
    'Retention: for the life of the tenant. A retired release stays, because instants, '
    'epochs and results bind its ID. Time deletes none.';
COMMENT ON TABLE time.acquisition IS
    'Retention: for the life of the tenant. A finished acquisition stays as the provenance '
    'of the release it staged. Time deletes none.';
COMMENT ON TABLE time.active_authority IS
    'Retention: one row per tenant and family for the life of the tenant. Activation '
    'replaces the row in place.';
COMMENT ON TABLE time.calendar_version IS
    'Retention: for the life of the tenant. Calendar versions are immutable, and Time '
    'deletes none.';
COMMENT ON TABLE time.epoch IS
    'Retention: for the life of the tenant. Epoch versions are immutable, and Time deletes '
    'none.';
COMMENT ON TABLE time.event IS
    'Retention: for the life of the tenant. Due and cancelled events stay for operator '
    'retirement. Time deletes none.';
COMMENT ON TABLE time.clock_policy IS
    'Retention: one row per tenant for the life of the tenant. Replacement updates the row '
    'in place.';
