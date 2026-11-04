-- veoveo: all tenants (the knowledge index embeds one event, whose key is unique across tenants)
-- The name of one event, which its knowledge index embeds. The key's UUIDv7 makes it
-- unique across tenants, which no index of `event_key` alone declares, so one row is
-- the bound.
SELECT name FROM time.event WHERE event_key = $1 LIMIT 1
