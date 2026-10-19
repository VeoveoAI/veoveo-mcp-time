// The runtime binds these identifiers to PostgreSQL `text`; contract consumers carry no
// database dependency.
#[cfg_attr(
    feature = "runtime",
    veoveo_types::id(prefixed(TimeIds, "time-source-"), postgres)
)]
#[cfg_attr(
    not(feature = "runtime"),
    veoveo_types::id(prefixed(TimeIds, "time-source-"))
)]
pub struct TimeSourceId(String);
#[cfg_attr(
    feature = "runtime",
    veoveo_types::id(prefixed(TimeIds, "time-release-"), postgres)
)]
#[cfg_attr(
    not(feature = "runtime"),
    veoveo_types::id(prefixed(TimeIds, "time-release-"))
)]
pub struct AuthorityReleaseId(String);
#[cfg_attr(
    feature = "runtime",
    veoveo_types::id(prefixed(TimeIds, "time-acquisition-"), postgres)
)]
#[cfg_attr(
    not(feature = "runtime"),
    veoveo_types::id(prefixed(TimeIds, "time-acquisition-"))
)]
pub struct TimeAcquisitionId(String);
#[cfg_attr(
    feature = "runtime",
    veoveo_types::id(prefixed(TimeIds, "calendar-"), postgres)
)]
#[cfg_attr(
    not(feature = "runtime"),
    veoveo_types::id(prefixed(TimeIds, "calendar-"))
)]
pub struct CalendarId(String);
#[cfg_attr(
    feature = "runtime",
    veoveo_types::id(prefixed(TimeIds, "epoch-"), postgres)
)]
#[cfg_attr(
    not(feature = "runtime"),
    veoveo_types::id(prefixed(TimeIds, "epoch-"))
)]
pub struct EpochId(String);
#[cfg_attr(
    feature = "runtime",
    veoveo_types::id(prefixed(TimeIds, "event-"), postgres)
)]
#[cfg_attr(
    not(feature = "runtime"),
    veoveo_types::id(prefixed(TimeIds, "event-"))
)]
pub struct EventId(String);

use veoveo_types::{IdProfile, IdProfileSpec};

const PUBLIC_ID_MAX_LENGTH: usize = 128;

#[doc(hidden)]
pub struct TimeIds;
impl IdProfile for TimeIds {
    type Error = String;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, metadata| validate_public_id(value, metadata.prefix))
            .owner_schema(public_id_schema, false);
}

fn validate_public_id(value: &str, prefix: &'static str) -> Result<(), String> {
    if value.len() < prefix.len() + 1
        || value.len() > PUBLIC_ID_MAX_LENGTH
        || !value.starts_with(prefix)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(format!("expected {} identifier", prefix));
    }
    Ok(())
}

fn public_id_schema(
    _: &mut schemars::SchemaGenerator,
    metadata: veoveo_types::IdMetadata,
) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "pattern": format!("^{}[A-Za-z0-9_.:-]+(?![\\s\\S])", metadata.prefix),
        "minLength": metadata.prefix.len() + 1,
        "maxLength": PUBLIC_ID_MAX_LENGTH,
        "description": format!(
            "Identifier beginning with {} followed by one or more ASCII letters, digits, hyphens, underscores, periods or colons; at most {} characters in total.",
            metadata.prefix, PUBLIC_ID_MAX_LENGTH
        ),
        "examples": [format!("{}example", metadata.prefix)]
    })
}
