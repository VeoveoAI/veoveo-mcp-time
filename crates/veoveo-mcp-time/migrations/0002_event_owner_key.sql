-- Event ID completion reads one owner's keys from a prefix range, which this index seeks.
CREATE INDEX event_owner_key ON time.event (tenant, owner, event_key);
