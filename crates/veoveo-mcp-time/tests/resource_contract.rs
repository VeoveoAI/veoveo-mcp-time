use veoveo_mcp_time::contract::{
    AuthorityReleaseId, CalendarId, EpochId, EventId, TimeDocument, TimeResource, TimeVersion,
    TimeZoneId,
};
use veoveo_types::{ResourceAddress, ResourceUri};
fn version() -> TimeVersion {
    TimeVersion::new(12).unwrap()
}

#[test]
fn all_resource_families_preserve_the_published_wire_shape() {
    let examples = [
        (TimeResource::Docs, "time://docs"),
        (
            TimeResource::Document(TimeDocument::Agents),
            "time://docs/agents",
        ),
        (
            TimeResource::Document(TimeDocument::Design),
            "time://docs/design",
        ),
        (TimeResource::Contract, "time://contract"),
        (TimeResource::ClockCurrent, "time://clock/current"),
        (TimeResource::ClockQuality, "time://clock/quality"),
        (
            TimeResource::AuthoritiesCurrent,
            "time://authorities/current",
        ),
        (
            TimeResource::AuthorityRelease(AuthorityReleaseId::parse("time-release-iana").unwrap()),
            "time://authorities/releases/time-release-iana",
        ),
        (
            TimeResource::Zone(TimeZoneId::parse("America/New_York").unwrap()),
            "time://zones/America/New_York",
        ),
        (
            TimeResource::Zone(TimeZoneId::parse("Etc/GMT+5").unwrap()),
            "time://zones/Etc/GMT+5",
        ),
        (TimeResource::Calendars, "time://calendars"),
        (
            TimeResource::Calendar {
                id: CalendarId::parse("calendar-one").unwrap(),
                version: version(),
            },
            "time://calendars/calendar-one/versions/12",
        ),
        (TimeResource::Epochs, "time://epochs"),
        (
            TimeResource::Epoch(EpochId::parse("epoch-one").unwrap()),
            "time://epochs/epoch-one",
        ),
        (TimeResource::Events, "time://events"),
        (
            TimeResource::Event(EventId::parse("event-one").unwrap()),
            "time://events/event-one",
        ),
    ];
    for (address, wire) in examples {
        assert_eq!(address.to_uri().unwrap().as_str(), wire);
        assert_eq!(TimeResource::parse(wire).unwrap(), address);
        assert_eq!(
            <TimeResource as ResourceAddress>::parse(&ResourceUri::new(wire).unwrap()).unwrap(),
            address
        );
        assert_eq!(serde_json::to_value(&address).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<TimeResource>(wire.into()).unwrap(),
            address
        );
    }
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(TimeResource)).unwrap()["type"],
        "string"
    );
}

#[test]
fn routes_reject_wrong_ids_aliases_and_unsupported_components() {
    for wire in [
        "time://calendars/epoch-one/versions/1",
        "time://calendars/calendar-one/versions/0",
        "time://calendars/calendar-one/versions/01",
        "time://calendars/calendar-one/versions/9223372036854775808",
        "time://calendars/calendar-one/versions/1?cursor=00",
        "time://calendars?",
        "time://calendars?offset=1",
        "time://epochs/calendar-one",
        "time://events/epoch-one",
        "time://events/event-one/extra",
        "time://events/event-%6fne",
        "time://events/event-one%2Fextra",
        "time://events/event-one#fragment",
        "time://events/../events/event-one",
        "time://events/{event_id}",
        "time://events/",
        "time://docs/unknown",
        "time://zones/America%2FNew_York",
        "time://zones//UTC",
        "time://zones/UTC?extra=1",
        "time://zones/Europe/../UTC",
        "time://zones/Etc/GMT%2B5",
        "time://zones",
        "time://zones/",
        "other://epochs/epoch-one",
        "ui://other/timeline.html",
    ] {
        assert!(TimeResource::parse(wire).is_err(), "accepted {wire}");
    }
    for name in [
        "", "/UTC", "UTC/", "../UTC", "UTC//GMT", "UTC?x", "UTC\n", "bad%zone",
    ] {
        assert!(TimeZoneId::parse(name).is_err());
    }
    assert!(TimeVersion::new(0).is_err());
    assert!(TimeVersion::new(u64::MAX).is_err());
}

#[test]
fn subscriptions_share_resource_validation_and_exclude_cursor_pages() {
    for wire in [
        "time://clock/current",
        "time://clock/quality",
        "time://authorities/current",
        "time://calendars",
        "time://epochs",
        "time://events",
        "time://events/event-one",
        "time://epochs/epoch-one",
        "time://calendars/calendar-one/versions/1",
    ] {
        assert!(TimeResource::parse(wire).unwrap().is_subscribable());
    }
    for wire in [
        "time://docs",
        "time://contract",
        "time://docs/agents",
        "time://zones/UTC",
        "time://authorities/releases/time-release-one",
    ] {
        assert!(!TimeResource::parse(wire).unwrap().is_subscribable());
    }
}

#[test]
fn version_schema_enforces_the_json_integer_range() {
    let schema = serde_json::to_value(schemars::schema_for!(TimeVersion)).unwrap();
    assert_eq!(schema["minimum"], 1);
    assert_eq!(schema["maximum"], veoveo_mcp_time::JSON_INTEGER_MAX);
    let max = veoveo_mcp_time::JSON_INTEGER_MAX as i128;
    assert!(serde_json::from_value::<TimeVersion>(serde_json::json!(max)).is_ok());
    for version in [0, -1, max + 1, i64::MAX as i128, i64::MAX as i128 + 1] {
        assert!(serde_json::from_value::<TimeVersion>(serde_json::json!(version)).is_err());
    }
    let parsed: TimeVersion = serde_json::from_value(serde_json::json!(12)).unwrap();
    assert_eq!(parsed, version());
}

#[test]
fn route_patterns_validate_encoded_builder_output_without_claiming_domain_admission() {
    let zone_route = &TimeResource::RESOURCE_ROUTES[10];
    let schema =
        serde_json::json!({"type": "string", "pattern": zone_route.wire_pattern().unwrap()});
    let validator = jsonschema::validator_for(&schema).unwrap();
    for name in ["UTC", "America/New_York", "Etc/GMT+5"] {
        let wire = TimeResource::Zone(TimeZoneId::parse(name).unwrap())
            .to_uri()
            .unwrap();
        assert!(
            validator.is_valid(&serde_json::json!(wire.as_str())),
            "{wire}"
        );
    }
    for wire in ["other://zones/UTC", "time://events/event-one"] {
        assert!(!validator.is_valid(&serde_json::json!(wire)), "{wire}");
    }
    // The structural tail permits empty captures; the owner requires a zone name.
    assert!(validator.is_valid(&serde_json::json!("time://zones")));
    assert!(TimeResource::parse("time://zones").is_err());
    // The structural route pattern does not decide TZDB name admission or aliases.
    let alias = "time://zones/America%2FNew_York";
    assert!(validator.is_valid(&serde_json::json!(alias)));
    assert!(TimeResource::parse(alias).is_err());
}

#[test]
fn collection_uris_use_shared_selection_admission_and_native_query_templates() {
    use veoveo_hub_sdk::{
        calls::ResourceRead,
        definition::{CollectionRequest, CollectionResource, ReadCollection, collection_template},
    };
    use veoveo_mcp_time::{Calendars, ReadCalendars};
    use veoveo_types::{CursorToken, PageSize};
    let cursor: CursorToken<Calendars> =
        serde_json::from_value(serde_json::json!("opaque+query&value")).unwrap();
    let read = ReadCollection::new(ReadCalendars, PageSize::new(2).unwrap()).continuing(cursor);
    let uri = read.uri().unwrap();
    let (base, limit, cursor) =
        CollectionRequest::<Calendars>::split_uri(&uri, ReadCalendars::default_limit()).unwrap();
    assert_eq!(
        TimeResource::parse(base.as_str()).unwrap(),
        TimeResource::Calendars
    );
    assert_eq!(limit.get(), 2);
    assert_eq!(cursor.unwrap().as_str(), "opaque+query&value");
    assert_eq!(
        collection_template(ReadCalendars::TEMPLATE)
            .unwrap()
            .as_str(),
        "time://calendars{?limit,cursor}"
    );
    for query in [
        "limit=0",
        "limit=101",
        "cursor=",
        "cursor=a&cursor=b",
        "cursor=a&%63ursor=b",
    ] {
        let uri = ResourceUri::new(format!("time://calendars?{query}")).unwrap();
        assert!(
            CollectionRequest::<Calendars>::split_uri(&uri, ReadCalendars::default_limit())
                .is_err(),
            "{uri}"
        );
    }
}
