use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use veoveo_mcp_time::*;

fn check<T: DeserializeOwned + Serialize + JsonSchema>(mut wire: Value, field: &str) {
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    assert_eq!(schema["properties"][field]["minimum"], 1);
    assert_eq!(schema["properties"][field]["maximum"], i64::MAX);
    for version in [1, i64::MAX as u64] {
        wire[field] = version.into();
        let value: T = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }
    for invalid in [
        json!(0),
        json!(-1),
        json!(i64::MAX as u64 + 1),
        json!(u64::MAX),
        json!(1.5),
        json!("1"),
        Value::Null,
    ] {
        wire[field] = invalid;
        assert!(serde_json::from_value::<T>(wire.clone()).is_err());
    }
    wire.as_object_mut().unwrap().remove(field);
    assert!(serde_json::from_value::<T>(wire).is_err());
}

fn source() -> Value {
    json!({"sourceId":"time-source-fixture", "name":"fixture", "datasetKind":"tzdb",
        "url":"https://example.test/data", "expectedContentType":"application/gzip",
        "enabled":true, "recordVersion":1})
}

fn instant() -> Value {
    json!({"taiSecondsSince1970":0, "nanosecond":0, "uncertaintyNanoseconds":0,
        "authority":{"tzdbReleaseId":"time-release-tzdb", "leapSecondsReleaseId":"time-release-leaps"}})
}

#[test]
fn lifecycle_metadata_admits_only_positive_storage_versions() {
    check::<TimeSource>(source(), "recordVersion");
    check::<AuthorityRelease>(
        json!({"releaseId":"time-release-fixture", "sourceId":"time-source-fixture",
        "datasetKind":"tzdb", "versionLabel":"fixture", "sourceUrl":"https://example.test/data",
        "sourceDigest":format!("sha256:{}", "a".repeat(64)), "artifactPath":"/tmp/fixture", "state":"staged",
        "retrievedAt":"2026-01-01T00:00:00Z", "validatedAt":"2026-01-01T00:00:00Z", "recordVersion":1}),
        "recordVersion",
    );
    check::<TimeAcquisition>(
        json!({"acquisitionId":"time-acquisition-fixture", "sourceId":"time-source-fixture",
        "expectedSourceDigest":null, "status":"queued", "phase":"queued", "stagedReleaseId":null,
        "message":"", "createdAt":"2026-01-01T00:00:00Z", "updatedAt":"2026-01-01T00:00:00Z", "recordVersion":1}),
        "recordVersion",
    );
    check::<Event>(
        json!({"eventId":"event-fixture", "name":"fixture", "due":instant(),
        "state":"scheduled", "recordVersion":1}),
        "recordVersion",
    );
}

#[test]
fn immutable_calendar_and_epoch_versions_share_the_checked_type() {
    check::<OperationalCalendar>(
        json!({"calendarId":"calendar-fixture", "version":1,
        "name":"fixture", "zoneId":"UTC", "windows":[], "excludedDates":[]}),
        "version",
    );
    check::<Epoch>(
        json!({"epochId":"epoch-fixture", "name":"fixture", "instant":instant(), "version":1}),
        "version",
    );
}

#[test]
fn source_creation_keeps_zero_and_cannot_decode_persisted_metadata() {
    let mut wire = source();
    wire["recordVersion"] = 0.into();
    let value: NewTimeSource = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(value.record_version, SourceCreationVersion);
    assert_eq!(serde_json::to_value(value).unwrap(), wire);
    assert!(serde_json::from_value::<TimeSource>(wire.clone()).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(NewTimeSource)).unwrap();
    assert_eq!(schema["properties"]["recordVersion"]["const"], 0);
    for version in [
        json!(1),
        json!(-1),
        json!(u64::MAX),
        json!("0"),
        Value::Null,
    ] {
        wire["recordVersion"] = version;
        assert!(serde_json::from_value::<NewTimeSource>(wire.clone()).is_err());
        assert!(
            serde_json::from_value::<CreateSourceRequest>(
                json!({"source":wire, "idempotencyKey":"fixture"})
            )
            .is_err()
        );
    }
    wire.as_object_mut().unwrap().remove("recordVersion");
    assert!(serde_json::from_value::<NewTimeSource>(wire).is_err());
}

#[test]
fn recorded_timestamps_build_at_microsecond_precision_and_decode_only_at_it() {
    let precise = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00.123456789Z")
        .unwrap()
        .to_utc();
    let release: AuthorityRelease =
        serde_json::from_value(active_selection()["release"].clone()).unwrap();
    let mut draft = AuthorityReleaseValue::from(release);
    draft.retrieved_at = precise;
    draft.validated_at = precise;
    let built = draft.build().unwrap();
    assert_eq!(
        built.retrieved_at,
        contract::admission::stored_timestamp(precise)
    );
    let wire = serde_json::to_value(&built).unwrap();
    assert_eq!(wire["retrievedAt"], "2026-01-01T00:00:00.123456Z");
    assert_eq!(wire["validatedAt"], "2026-01-01T00:00:00.123456Z");
    assert_eq!(
        serde_json::from_value::<AuthorityRelease>(wire.clone()).unwrap(),
        built
    );
    for (field, value) in [
        ("retrievedAt", "2026-01-01T00:00:00.123455999Z"),
        ("validatedAt", "2026-01-01T00:00:00.123456789Z"),
    ] {
        let mut nanoseconds = wire.clone();
        nanoseconds[field] = value.into();
        assert!(serde_json::from_value::<AuthorityRelease>(nanoseconds).is_err());
    }
    let acquisition = json!({"acquisitionId":"time-acquisition-fixture",
        "sourceId":"time-source-fixture", "expectedSourceDigest":null, "status":"queued",
        "phase":"queued", "stagedReleaseId":null, "message":"",
        "createdAt":"2026-01-01T00:00:00.123456Z", "updatedAt":"2026-01-01T00:00:00.123457Z",
        "recordVersion":1});
    assert!(serde_json::from_value::<TimeAcquisition>(acquisition.clone()).is_ok());
    for field in ["createdAt", "updatedAt"] {
        let mut nanoseconds = acquisition.clone();
        nanoseconds[field] = "2026-01-01T00:00:00.1234565Z".into();
        assert!(serde_json::from_value::<TimeAcquisition>(nanoseconds).is_err());
    }
}

fn active_selection() -> Value {
    json!({"pointerVersion":1,"release":{
        "releaseId":"time-release-fixture", "sourceId":"time-source-fixture",
        "datasetKind":"tzdb", "versionLabel":"fixture", "sourceUrl":"https://example.test/data",
        "sourceDigest":format!("sha256:{}", "a".repeat(64)), "artifactPath":"/tmp/fixture", "state":"active",
        "retrievedAt":"2026-01-01T00:00:00Z", "validatedAt":"2026-01-01T00:00:00Z", "recordVersion":2
    }})
}
#[test]
fn active_selection_exposes_pointer_guard_and_rejects_unadmitted_wire() {
    let wire = active_selection();
    let selection: ActiveAuthoritySelection = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(selection.pointer_version.get(), 1);
    assert_eq!(selection.release.record_version.get(), 2);
    assert_eq!(
        selection.write_guard(),
        TimeWriteGuard::Existing(TimeVersion::FIRST)
    );
    assert_eq!(serde_json::to_value(&selection).unwrap(), wire);
    check::<ActiveAuthoritySelection>(wire.clone(), "pointerVersion");
    let schema = serde_json::to_value(schemars::schema_for!(ActiveAuthoritySelection)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&wire));
    for invalid in [
        {
            let mut value = wire.clone();
            value["pointerVersion"] = json!(0);
            value
        },
        {
            let mut value = wire.clone();
            value["release"]["state"] = json!("staged");
            value
        },
        {
            let mut value = wire.clone();
            value["release"]["state"] = json!("retired");
            value
        },
        {
            let mut value = wire.clone();
            value["unexpected"] = json!(true);
            value
        },
        wire["release"].clone(),
    ] {
        assert!(!validator.is_valid(&invalid));
        assert!(serde_json::from_value::<ActiveAuthoritySelection>(invalid).is_err());
    }
    let mut staged: AuthorityReleaseValue = selection.release.clone().into();
    staged.state = AuthorityReleaseState::Staged;
    assert!(
        ActiveAuthoritySelectionValue {
            pointer_version: TimeVersion::FIRST,
            release: staged.build().unwrap()
        }
        .build()
        .is_err()
    );
    let page = veoveo_types::Pager::<ActiveAuthorities>::new(
        "fixture",
        veoveo_hub_sdk::definition::NoFilters {},
        veoveo_types::PageSize::new(100).unwrap(),
        veoveo_types::LiveKeyset,
        None,
    )
    .unwrap()
    .finish(vec![selection])
    .unwrap();
    let wire = serde_json::to_value(page).unwrap();
    let admitted: veoveo_types::Page<ActiveAuthorities> =
        serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(admitted.items().len(), 1);
    let validator = jsonschema::validator_for(
        &serde_json::to_value(schemars::schema_for!(veoveo_types::Page<ActiveAuthorities>))
            .unwrap(),
    )
    .unwrap();
    assert!(validator.is_valid(&wire));
    let old = json!({"items":[wire["items"][0]["release"].clone()],"nextCursor":null});
    assert!(!validator.is_valid(&old));
    assert!(serde_json::from_value::<veoveo_types::Page<ActiveAuthorities>>(old).is_err());
}
