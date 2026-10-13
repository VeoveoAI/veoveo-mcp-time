//! Completion candidates: distinct stored values that start with the typed prefix.
use super::*;

/// The keys that start with a completion prefix, as the half-open range `[lower, upper)`
/// that a `COLLATE "C"` index seeks. Stored keys are lowercase printable ASCII, so the
/// prefix matches case-insensitively and a prefix with any other character matches none.
/// Comparison treats every character literally; no character is a wildcard.
#[derive(Debug, PartialEq, Eq)]
struct KeyRange {
    lower: String,
    upper: String,
}

impl KeyRange {
    fn starting_with(prefix: &str) -> Option<Self> {
        let lower = prefix.to_ascii_lowercase();
        if !lower.bytes().all(|byte| byte.is_ascii_graphic()) {
            return None;
        }
        let mut upper = lower.clone().into_bytes();
        match upper.last_mut() {
            // A printable byte is at most `~`, so its successor is still ASCII.
            Some(last) => *last += 1,
            // Every printable ASCII key sorts below DEL.
            None => upper.push(0x7f),
        }
        let upper = String::from_utf8(upper).expect("an ASCII range bound");
        Some(Self { lower, upper })
    }
}

impl TimePersistence {
    pub(crate) async fn complete_time_values(
        &self,
        identity: &PlatformIdentity,
        domain: TimeCompletion,
        prefix: &str,
        limit: u32,
    ) -> Result<Vec<String>, PersistenceError> {
        if !(1..=101).contains(&limit) {
            return Err(invalid("limit", "must be in 1..=101"));
        }
        let limit = i64::from(limit);
        let tenant = identity.tenant_id;
        let keys = || KeyRange::starting_with(prefix);
        let candidates = match domain {
            TimeCompletion::CalendarId => {
                let Some(keys) = keys() else {
                    return Ok(Vec::new());
                };
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_calendar_id.sql",
                    tenant as TenantId,
                    keys.lower,
                    keys.upper,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::CalendarVersion {
                calendar_key: Some(calendar_key),
            } => {
                validate_key("calendar_key", &calendar_key, "calendar-")?;
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_calendar_version_of.sql",
                    tenant as TenantId,
                    &calendar_key as &CalendarId,
                    prefix,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::CalendarVersion { calendar_key: None } => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_calendar_version.sql",
                    tenant as TenantId,
                    prefix,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::EpochId => {
                let Some(keys) = keys() else {
                    return Ok(Vec::new());
                };
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_epoch_id.sql",
                    tenant as TenantId,
                    keys.lower,
                    keys.upper,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::EpochVersion {
                epoch_key: Some(epoch_key),
            } => {
                validate_key("epoch_key", &epoch_key, "epoch-")?;
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_epoch_version_of.sql",
                    tenant as TenantId,
                    &epoch_key as &EpochId,
                    prefix,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::EpochVersion { epoch_key: None } => {
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_epoch_version.sql",
                    tenant as TenantId,
                    prefix,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::AuthorityReleaseId => {
                let Some(keys) = keys() else {
                    return Ok(Vec::new());
                };
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_release_id.sql",
                    tenant as TenantId,
                    keys.lower,
                    keys.upper,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
            TimeCompletion::EventId => {
                let Some(keys) = keys() else {
                    return Ok(Vec::new());
                };
                sqlx::query_file_scalar!(
                    "src/persistence/queries/complete_event_id.sql",
                    tenant as TenantId,
                    identity.principal_id as PrincipalId,
                    keys.lower,
                    keys.upper,
                    limit,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::KeyRange;

    fn range(prefix: &str) -> Option<(String, String)> {
        KeyRange::starting_with(prefix).map(|range| (range.lower, range.upper))
    }

    #[test]
    fn a_prefix_selects_exactly_the_keys_that_start_with_it() {
        assert_eq!(range(""), Some((String::new(), "\u{7f}".into())));
        assert_eq!(
            range("Calendar-0"),
            Some(("calendar-0".into(), "calendar-1".into()))
        );
        assert_eq!(
            range("event-~"),
            Some(("event-~".into(), "event-\u{7f}".into()))
        );
        // LIKE metacharacters are ordinary bytes of the range.
        assert_eq!(range("%"), Some(("%".into(), "&".into())));
        assert_eq!(range("epoch_"), Some(("epoch_".into(), "epoch`".into())));
        for prefix in ["calendar ", "événement", "event-\0", "line\nbreak"] {
            assert_eq!(range(prefix), None, "{prefix:?}");
        }
        let keys = [
            "calendar-0198",
            "calendar-0",
            "calendar-0/",
            "calendar-1",
            "calendar-",
            "calendar_0",
        ];
        let (lower, upper) = range("calendar-0").unwrap();
        let selected: Vec<_> = keys
            .iter()
            .filter(|key| lower.as_str() <= **key && **key < upper.as_str())
            .collect();
        assert_eq!(selected, [&"calendar-0198", &"calendar-0", &"calendar-0/"]);
    }
}
