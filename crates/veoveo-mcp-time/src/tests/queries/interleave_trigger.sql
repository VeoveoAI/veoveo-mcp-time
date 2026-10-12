-- Runs the current `time.test_interleave` body inside an activation, right after the
-- candidate becomes active and before the pointer and previous release change.
CREATE TRIGGER test_interleave AFTER UPDATE ON time.authority_release
FOR EACH ROW WHEN (OLD.state = 'staged' AND NEW.state = 'active')
EXECUTE FUNCTION time.test_interleave()
