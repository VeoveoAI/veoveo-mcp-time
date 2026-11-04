-- The knowledge contract through the SDK's PostgreSQL search: an event's words, its
-- vector and the model that made it, and the read rule its search shares with the event
-- listing. pgvector lives in the installation's `_extensions` schema.

-- Its name, which PostgreSQL keeps current.
ALTER TABLE time.event
    ADD COLUMN knowledge_text tsvector GENERATED ALWAYS AS (to_tsvector('simple', name)) STORED,
    ADD COLUMN knowledge_vector _extensions.vector(768),
    ADD COLUMN knowledge_model text CHECK (time.is_text(knowledge_model, 512));

CREATE INDEX event_knowledge_text ON time.event USING gin (knowledge_text);
CREATE INDEX event_knowledge_vector ON time.event
    USING hnsw (knowledge_vector _extensions.vector_cosine_ops);
-- An event's key is unique across tenants; the search's index reads and writes one event
-- by it alone. A hash index serves that equality and leaves key ranges, which ID
-- completion seeks per owner, to `event_owner_key`.
CREATE INDEX event_knowledge_key ON time.event USING hash (event_key);

-- Whether the search may show `e` to `caller`, the SDK's `KnowledgeCaller`: an event of
-- the caller's tenant that the caller owns, as the event listing decides.
-- `time` is also a type name, so the row type is quoted.
CREATE FUNCTION time.knowledge_readable(e "time".event, caller jsonb) RETURNS boolean
LANGUAGE sql STABLE PARALLEL SAFE
SET search_path = pg_catalog, pg_temp
RETURN e.tenant = (caller->>'tenantId')::uuid
    AND e.owner = (caller->>'principalId')::uuid;
