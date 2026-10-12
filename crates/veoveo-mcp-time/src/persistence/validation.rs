use super::*;
use uuid::Uuid;

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const KEY_REASON: &str = "must use the canonical prefix followed by a lowercase UUIDv7";

pub(super) fn validate_source(draft: &TimeSourceDraft) -> Result<(), PersistenceError> {
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("name", &draft.name, 256)?;
    validate_https_url("source_url", &draft.source_url)?;
    validate_text("expected_content_type", &draft.expected_content_type, 128)?;
    validate_body(&draft.body)
}

pub(super) fn validate_release(draft: &TimeAuthorityReleaseDraft) -> Result<(), PersistenceError> {
    validate_key("release_key", &draft.release_key, "time-release-")?;
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("version_label", &draft.version_label, 256)?;
    validate_https_url("source_url", &draft.source_url)?;
    validate_absolute_path("artifact_path", &draft.artifact_path)?;
    validate_timestamp("retrieved_at", &draft.retrieved_at)?;
    validate_timestamp("validated_at", &draft.validated_at)?;
    if draft.validated_at < draft.retrieved_at {
        return Err(invalid("validated_at", "must not precede retrieval"));
    }
    validate_body(&draft.body)
}

pub(super) fn validate_acquisition(draft: &TimeAcquisitionDraft) -> Result<(), PersistenceError> {
    validate_key(
        "acquisition_key",
        &draft.acquisition_key,
        "time-acquisition-",
    )?;
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("idempotency_key", &draft.idempotency_key, 256)?;
    if let Some(release) = &draft.staged_release_key {
        validate_key("staged_release_key", release, "time-release-")?;
    }
    validate_body(&draft.body)
}

pub(super) fn validate_event(draft: &TimeEventDraft) -> Result<(), PersistenceError> {
    validate_key("event_key", &draft.event_key, "event-")?;
    validate_text("name", &draft.name, 256)?;
    validate_text("idempotency_key", &draft.idempotency_key, 256)?;
    validate_timestamp("due", &draft.due)?;
    validate_body(&draft.body)
}

/// A bound timestamp already at the column's microsecond precision, so the column
/// holds exactly the instant the body names.
pub(super) fn validate_timestamp(
    field: &'static str,
    value: &DateTime<Utc>,
) -> Result<(), PersistenceError> {
    crate::contract::admission::stored_precision(value)
        .map_err(|_| invalid(field, "must carry at most microsecond precision"))
}

/// A stored key: its prefix, then a lowercase hyphenated RFC 9562 version 7 UUID. The
/// schema's `time.is_key` check admits the same form.
pub(crate) fn validate_key(
    field: &'static str,
    value: impl AsRef<str>,
    prefix: &'static str,
) -> Result<(), PersistenceError> {
    let raw = value
        .as_ref()
        .strip_prefix(prefix)
        .ok_or_else(|| invalid(field, KEY_REASON))?;
    let uuid = Uuid::try_parse(raw).map_err(|_| invalid(field, KEY_REASON))?;
    if uuid.get_version_num() != 7
        || uuid.get_variant() != uuid::Variant::RFC4122
        || uuid.hyphenated().to_string() != raw
    {
        return Err(invalid(field, KEY_REASON));
    }
    Ok(())
}

pub(super) fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), PersistenceError> {
    crate::contract::admission::text(value, max).map_err(|_| {
        invalid(
            field,
            "must be non-empty, bounded, and contain no control characters",
        )
    })
}

pub(super) fn validate_positive(field: &'static str, value: i64) -> Result<(), PersistenceError> {
    if value < 1 {
        return Err(invalid(field, "must be positive"));
    }
    Ok(())
}

pub(super) fn validate_zone_id(value: &str) -> Result<(), PersistenceError> {
    crate::contract::admission::zone(value)
        .map_err(|_| invalid("zone_id", "must be a bounded IANA zone identifier"))
}

pub(super) fn validate_https_url(field: &'static str, value: &str) -> Result<(), PersistenceError> {
    crate::contract::admission::https(value).map_err(|_| {
        invalid(
            field,
            "must be an absolute HTTPS URL without credentials or a fragment",
        )
    })
}

pub(super) fn validate_absolute_path(
    field: &'static str,
    value: &str,
) -> Result<(), PersistenceError> {
    crate::contract::admission::absolute_path(value)
        .map_err(|_| invalid(field, "must be a confined absolute path"))
}

/// The JSON document a row stores: an object of at most 2 MiB.
pub(super) fn validate_body(value: &serde_json::Value) -> Result<(), PersistenceError> {
    if !value.is_object() || body_bytes(value) > MAX_BODY_BYTES {
        return Err(invalid("body", "must be a JSON object of at most 2 MiB"));
    }
    Ok(())
}

pub(super) fn body_bytes(value: &serde_json::Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_time_keys_require_a_lowercase_uuid_v7() {
        let key = Uuid::now_v7();
        assert!(validate_key("event_key", format!("event-{key}"), "event-").is_ok());
        assert!(validate_key("event_key", format!("event-{}", Uuid::new_v4()), "event-").is_err());
        for alias in [
            key.hyphenated().to_string().to_uppercase(),
            key.simple().to_string(),
            key.braced().to_string(),
            key.urn().to_string(),
        ] {
            assert!(validate_key("event_key", format!("event-{alias}"), "event-").is_err());
        }
    }

    #[test]
    fn named_bootstrap_references_keep_the_public_profile_without_becoming_stored_keys() {
        let bootstrap = AuthorityReleaseId::parse("time-release-tzdb-bootstrap").unwrap();
        assert!(validate_key("release_key", &bootstrap, "time-release-").is_err());
        assert_eq!(
            serde_json::from_str::<AuthorityReleaseId>(&serde_json::to_string(&bootstrap).unwrap())
                .unwrap(),
            bootstrap
        );
        let stored = AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7())).unwrap();
        assert!(validate_key("release_key", &stored, "time-release-").is_ok());
    }

    #[test]
    fn authority_sources_require_safe_https_urls() {
        assert!(
            validate_https_url(
                "source_url",
                "https://data.iana.org/time-zones/tzdata-latest.tar.gz"
            )
            .is_ok()
        );
        assert!(validate_https_url("source_url", "http://example.test/tzdb").is_err());
        assert!(validate_https_url("source_url", "https://user@example.test/tzdb").is_err());
    }

    #[test]
    fn bodies_are_bounded_objects() {
        assert!(validate_body(&serde_json::json!({"state": "staged"})).is_ok());
        assert!(validate_body(&serde_json::json!(["state"])).is_err());
        let large = "x".repeat(MAX_BODY_BYTES);
        assert!(validate_body(&serde_json::json!({ "value": large })).is_err());
    }
}
