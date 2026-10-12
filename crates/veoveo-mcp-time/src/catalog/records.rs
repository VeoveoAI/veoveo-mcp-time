//! Checked conversion of SQL-selected records into the public Time model.
use super::{
    acquisition_state_from_store, event_state_from_store, release_state_from_store, source_kind,
};
use crate::contract::{
    AuthorityRelease, Epoch, Event, OperationalCalendar, TimeAcquisition, TimeSource, TimeVersion,
};
use crate::persistence::{
    TimeAcquisitionRecord, TimeAuthorityReleaseRecord, TimeCalendarVersionRecord, TimeEpochRecord,
    TimeEventRecord, TimeSourceRecord, validate_key,
};
use anyhow::Result;
use serde::de::DeserializeOwned;

#[derive(Clone, Copy, Debug)]
enum Entity {
    Source,
    Release,
    Acquisition,
    Calendar,
    Epoch,
    Event,
}

impl Entity {
    fn prefix(self) -> &'static str {
        match self {
            Self::Source => "time-source-",
            Self::Release => "time-release-",
            Self::Acquisition => "time-acquisition-",
            Self::Calendar => "calendar-",
            Self::Epoch => "epoch-",
            Self::Event => "event-",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid stored Time {entity:?} metadata in {field}; inspect the retained record")]
struct StoredMetadataError {
    entity: Entity,
    field: &'static str,
}

fn invalid(entity: Entity, field: &'static str) -> StoredMetadataError {
    StoredMetadataError { entity, field }
}

fn check(entity: Entity, field: &'static str, valid: bool) -> Result<()> {
    if !valid {
        return Err(invalid(entity, field).into());
    }
    Ok(())
}

fn body<T: DeserializeOwned>(entity: Entity, json: serde_json::Value) -> Result<T> {
    // Serde's diagnostic may quote a stored value. Public errors name only the field.
    serde_json::from_value(json).map_err(|_| invalid(entity, "body").into())
}

fn version(entity: Entity, field: &'static str, value: i64) -> Result<TimeVersion> {
    u64::try_from(value)
        .ok()
        .and_then(|value| TimeVersion::new(value).ok())
        .ok_or_else(|| invalid(entity, field).into())
}

fn identity(entity: Entity, key: &str, body_key: &str) -> Result<()> {
    reference(entity, "identity", key, body_key, entity)
}

fn reference(
    entity: Entity,
    field: &'static str,
    key: &str,
    body_key: &str,
    target: Entity,
) -> Result<()> {
    check(
        entity,
        field,
        key == body_key && validate_key("key", key, target.prefix()).is_ok(),
    )
}

pub(super) fn source_from_record(record: TimeSourceRecord) -> Result<TimeSource> {
    let kind = Entity::Source;
    let mut value = crate::TimeSourceValue::from(body::<TimeSource>(kind, record.body)?);
    value.record_version = version(kind, "record_version", record.record_version)?;
    identity(kind, record.source_key.as_str(), value.source_id.as_str())?;
    check(kind, "name", value.name == record.name)?;
    check(
        kind,
        "dataset_kind",
        source_kind(value.dataset_kind) == record.dataset_kind,
    )?;
    check(kind, "source_url", value.url == record.source_url)?;
    check(
        kind,
        "expected_content_type",
        value.expected_content_type == record.expected_content_type,
    )?;
    check(kind, "enabled", value.enabled == record.enabled)?;
    Ok(value.build()?)
}

pub(super) fn release_from_record(record: TimeAuthorityReleaseRecord) -> Result<AuthorityRelease> {
    let kind = Entity::Release;
    let mut value =
        crate::AuthorityReleaseValue::from(body::<AuthorityRelease>(kind, record.body)?);
    value.record_version = version(kind, "record_version", record.record_version)?;
    identity(kind, record.release_key.as_str(), value.release_id.as_str())?;
    reference(
        kind,
        "source_key",
        record.source_key.as_str(),
        value.source_id.as_str(),
        Entity::Source,
    )?;
    check(
        kind,
        "dataset_kind",
        source_kind(value.dataset_kind) == record.dataset_kind,
    )?;
    check(
        kind,
        "version_label",
        value.version_label == record.version_label,
    )?;
    check(kind, "source_url", value.source_url == record.source_url)?;
    check(
        kind,
        "source_digest",
        value.source_digest.as_str() == record.source_digest,
    )?;
    check(
        kind,
        "artifact_path",
        value.artifact_path == record.artifact_path,
    )?;
    check(
        kind,
        "retrieved_at",
        value.retrieved_at == record.retrieved_at,
    )?;
    check(
        kind,
        "validated_at",
        value.validated_at == record.validated_at,
    )?;
    // Retirement changes these columns without rewriting the historical JSON body.
    value.state = release_state_from_store(record.state);
    Ok(value.build()?)
}

pub(super) fn acquisition_from_record(record: TimeAcquisitionRecord) -> Result<TimeAcquisition> {
    let kind = Entity::Acquisition;
    let mut value = body::<TimeAcquisition>(kind, record.body)?;
    value.record_version = version(kind, "record_version", record.record_version)?;
    identity(
        kind,
        record.acquisition_key.as_str(),
        value.acquisition_id.as_str(),
    )?;
    reference(
        kind,
        "source_key",
        record.source_key.as_str(),
        value.source_id.as_str(),
        Entity::Source,
    )?;
    check(
        kind,
        "expected_source_digest",
        value
            .expected_source_digest
            .as_ref()
            .map(|digest| digest.as_str())
            == record.expected_source_digest.as_deref(),
    )?;
    value.status = acquisition_state_from_store(record.status);
    value.phase = record.phase;
    if let Some(release) = &record.staged_release_key {
        check(
            kind,
            "staged_release_key",
            validate_key("key", release, Entity::Release.prefix()).is_ok(),
        )?;
    }
    value.staged_release_id = record.staged_release_key;
    value.updated_at = record.updated_at;
    veoveo_types::Check::check(&value)?;
    Ok(value)
}

pub(super) fn calendar_from_record(
    record: TimeCalendarVersionRecord,
) -> Result<OperationalCalendar> {
    let kind = Entity::Calendar;
    let value: OperationalCalendar = body(kind, record.body)?;
    identity(
        kind,
        record.calendar_key.as_str(),
        value.calendar_id.as_str(),
    )?;
    let version = version(kind, "calendar_version", record.calendar_version)?;
    check(kind, "calendar_version", value.version == version)?;
    check(kind, "name", value.name == record.name)?;
    check(kind, "zone_id", value.zone_id == record.zone_id)?;
    Ok(value)
}

pub(super) fn epoch_from_record(record: TimeEpochRecord) -> Result<Epoch> {
    let kind = Entity::Epoch;
    let value: Epoch = body(kind, record.body)?;
    identity(kind, record.epoch_key.as_str(), value.epoch_id.as_str())?;
    let version = version(kind, "epoch_version", record.epoch_version)?;
    check(kind, "epoch_version", value.version == version)?;
    check(kind, "name", value.name == record.name)?;
    check(
        kind,
        "tai_seconds_since_1970",
        value.instant.tai_seconds_since_1970 == record.tai_seconds_since_1970,
    )?;
    check(
        kind,
        "nanosecond",
        i64::from(value.instant.nanosecond.get()) == i64::from(record.nanosecond),
    )?;
    Ok(value)
}

pub(super) fn event_from_record(record: TimeEventRecord) -> Result<Event> {
    let kind = Entity::Event;
    let mut value = body::<Event>(kind, record.body)?;
    value.record_version = version(kind, "record_version", record.record_version)?;
    identity(kind, record.event_key.as_str(), value.event_id.as_str())?;
    check(kind, "name", value.name == record.name)?;
    check(
        kind,
        "due_tai_seconds_since_1970",
        value.due.tai_seconds_since_1970 == record.due_tai_seconds_since_1970,
    )?;
    check(
        kind,
        "due_nanosecond",
        i64::from(value.due.nanosecond.get()) == i64::from(record.due_nanosecond),
    )?;
    value.state = event_state_from_store(record.state);
    Ok(value)
}
