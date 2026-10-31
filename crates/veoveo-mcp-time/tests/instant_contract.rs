use serde_json::{Value, json};
use veoveo_mcp_time::{
    AuthorityBinding, AuthorityReleaseId, JSON_INTEGER_MAX, NanosecondBound, SubsecondNanoseconds,
    TimeCoordinateError, TimeExpression, TimeInstant, TimeSpan,
};

fn bound(nanoseconds: u64) -> NanosecondBound {
    NanosecondBound::bounded(nanoseconds).unwrap()
}

fn authority() -> AuthorityBinding {
    AuthorityBinding::new(
        AuthorityReleaseId::parse("time-release-tzdb").unwrap(),
        AuthorityReleaseId::parse("time-release-leaps").unwrap(),
    )
    .unwrap()
}

#[test]
fn subsecond_schema_and_numeric_admission_agree() {
    let schema = serde_json::to_value(schemars::schema_for!(SubsecondNanoseconds)).unwrap();
    assert_eq!(schema["type"], "integer");
    assert_eq!(schema["minimum"], 0);
    assert_eq!(schema["maximum"], 999_999_999);
    for nanos in [0, 1, 500_000_000, 999_999_999] {
        let value = SubsecondNanoseconds::new(nanos).unwrap();
        assert_eq!(value.get(), nanos);
        assert_eq!(serde_json::to_value(value).unwrap(), json!(nanos));
        assert_eq!(
            serde_json::from_value::<SubsecondNanoseconds>(json!(nanos)).unwrap(),
            value
        );
    }
    assert_eq!(SubsecondNanoseconds::default(), SubsecondNanoseconds::ZERO);
    for nanos in [1_000_000_000, u32::MAX] {
        assert_eq!(
            SubsecondNanoseconds::new(nanos),
            Err(TimeCoordinateError::InvalidNanosecond)
        );
    }
    for value in [
        json!(-1),
        json!(1_000_000_000),
        json!(u64::MAX),
        json!(0.5),
        json!("0"),
        Value::Null,
    ] {
        assert!(serde_json::from_value::<SubsecondNanoseconds>(value).is_err());
    }
}

#[test]
fn instant_and_expression_wires_keep_numeric_fractions_and_zero_defaults() {
    let mut instant = json!({"taiSecondsSince1970":-1, "nanosecond":999_999_999,
        "uncertainty":{"kind":"unbounded"}, "authority":authority()});
    let decoded: TimeInstant = serde_json::from_value(instant.clone()).unwrap();
    assert_eq!(decoded.total_nanoseconds(), -1);
    assert!(decoded.uncertainty.is_unbounded());
    assert_eq!(serde_json::to_value(decoded).unwrap(), instant);
    let schema = serde_json::to_value(schemars::schema_for!(TimeInstant)).unwrap();
    assert_eq!(schema["properties"]["nanosecond"]["maximum"], 999_999_999);
    for format in ["unix", "tai"] {
        let seconds = if format == "unix" {
            "seconds"
        } else {
            "secondsSince1970"
        };
        let wire = json!({"format":format, seconds:0});
        let decoded: TimeExpression = serde_json::from_value(wire.clone()).unwrap();
        let mut expected = wire;
        expected["nanosecond"] = 0.into();
        assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
        for invalid in [
            json!(-1),
            json!(1_000_000_000),
            json!(u32::MAX),
            json!(0.5),
            json!("0"),
            Value::Null,
        ] {
            instant["nanosecond"] = invalid.clone();
            expected["nanosecond"] = invalid;
            assert!(serde_json::from_value::<TimeInstant>(instant.clone()).is_err());
            assert!(serde_json::from_value::<TimeExpression>(expected.clone()).is_err());
        }
    }
    instant.as_object_mut().unwrap().remove("nanosecond");
    assert!(serde_json::from_value::<TimeInstant>(instant).is_err());
}

#[test]
fn total_coordinates_round_trip_negative_fractions_and_both_signed_endpoints() {
    let minimum = -i128::from(JSON_INTEGER_MAX) * 1_000_000_000;
    let maximum = i128::from(JSON_INTEGER_MAX) * 1_000_000_000 + 999_999_999;
    for total in [
        minimum,
        minimum + 1,
        -1_000_000_001,
        -1_000_000_000,
        -1,
        0,
        1,
        999_999_999,
        1_000_000_000,
        maximum,
    ] {
        let value = TimeInstant::from_total_nanoseconds(total, bound(42), authority()).unwrap();
        assert_eq!(value.total_nanoseconds(), total);
        assert_eq!(value.uncertainty, bound(42));
        assert_eq!(value.authority, authority());
        let wire = serde_json::to_value(&value).unwrap();
        assert_eq!(serde_json::from_value::<TimeInstant>(wire).unwrap(), value);
        let span = TimeSpan::from_total_nanoseconds(total).unwrap();
        assert_eq!(span.total_nanoseconds(), total);
    }
    let negative =
        TimeInstant::from_total_nanoseconds(-1, NanosecondBound::ZERO, authority()).unwrap();
    assert_eq!(negative.tai_seconds_since_1970, -1);
    assert_eq!(negative.nanosecond, SubsecondNanoseconds::MAX);
    for total in [i128::MIN, minimum - 1, maximum + 1, i128::MAX] {
        assert_eq!(
            TimeInstant::from_total_nanoseconds(total, NanosecondBound::ZERO, authority()),
            Err(TimeCoordinateError::SecondsOverflow)
        );
        assert_eq!(
            TimeSpan::from_total_nanoseconds(total),
            Err(TimeCoordinateError::SecondsOverflow)
        );
    }
}

#[test]
fn every_integer_seconds_field_keeps_within_the_json_range() {
    let edge = JSON_INTEGER_MAX;
    let instant = |seconds: Value| {
        json!({"taiSecondsSince1970":seconds,"nanosecond":0,
            "uncertainty":{"kind":"bounded","nanoseconds":0},"authority":authority()})
    };
    let span = |seconds: Value| json!({"seconds":seconds,"nanosecond":0});
    for seconds in [-edge, edge] {
        assert!(serde_json::from_value::<TimeInstant>(instant(json!(seconds))).is_ok());
        assert!(serde_json::from_value::<TimeSpan>(span(json!(seconds))).is_ok());
        for (format, field) in [("unix", "seconds"), ("tai", "secondsSince1970")] {
            let wire = json!({"format":format, field:seconds});
            assert!(serde_json::from_value::<TimeExpression>(wire).is_ok());
        }
    }
    for seconds in [-edge - 1, edge + 1, i64::MIN, i64::MAX] {
        assert!(serde_json::from_value::<TimeInstant>(instant(json!(seconds))).is_err());
        assert!(serde_json::from_value::<TimeSpan>(span(json!(seconds))).is_err());
        for (format, field) in [("unix", "seconds"), ("tai", "secondsSince1970")] {
            let wire = json!({"format":format, field:seconds});
            assert!(
                serde_json::from_value::<TimeExpression>(wire).is_err(),
                "{format} {seconds}"
            );
        }
    }
    // Encoding never emits seconds outside the range either.
    let mut outside =
        TimeInstant::from_total_nanoseconds(0, NanosecondBound::ZERO, authority()).unwrap();
    outside.tai_seconds_since_1970 = edge + 1;
    assert!(serde_json::to_value(&outside).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(TimeInstant)).unwrap();
    let seconds = &schema["properties"]["taiSecondsSince1970"];
    assert_eq!(seconds["minimum"], -edge);
    assert_eq!(seconds["maximum"], edge);
    let schema = serde_json::to_value(schemars::schema_for!(TimeSpan)).unwrap();
    assert_eq!(schema["properties"]["seconds"]["maximum"], edge);
    assert_eq!(schema["properties"]["nanosecond"]["maximum"], 999_999_999);
}

#[test]
fn a_nanosecond_bound_is_bounded_within_the_json_range_or_unbounded() {
    let max = NanosecondBound::MAX_NANOSECONDS;
    assert_eq!(max, JSON_INTEGER_MAX as u64);
    for (value, wire) in [
        (
            NanosecondBound::ZERO,
            json!({"kind":"bounded","nanoseconds":0}),
        ),
        (bound(max), json!({"kind":"bounded","nanoseconds":max})),
        (NanosecondBound::UNBOUNDED, json!({"kind":"unbounded"})),
    ] {
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<NanosecondBound>(wire).unwrap(),
            value
        );
    }
    assert!(NanosecondBound::bounded(max + 1).is_err());
    for refused in [
        json!({"kind":"bounded","nanoseconds":max + 1}),
        json!({"kind":"bounded","nanoseconds":-1}),
        json!({"kind":"bounded"}),
        json!({"kind":"unbounded","nanoseconds":0}),
        json!({"kind":"infinite"}),
        json!(0),
        Value::Null,
    ] {
        assert!(
            serde_json::from_value::<NanosecondBound>(refused.clone()).is_err(),
            "{refused}"
        );
    }
    assert_eq!(NanosecondBound::default(), NanosecondBound::ZERO);
    assert!(bound(max) < NanosecondBound::UNBOUNDED && bound(1) < bound(2));
    assert_eq!(bound(1).checked_add(bound(2)), Some(bound(3)));
    assert_eq!(bound(max).checked_add(bound(1)), None);
    assert_eq!(
        bound(max).checked_add(NanosecondBound::UNBOUNDED),
        Some(NanosecondBound::UNBOUNDED)
    );
    assert_eq!(bound(max).nanoseconds(), Some(max));
    assert_eq!(NanosecondBound::UNBOUNDED.nanoseconds(), None);
    let schema = serde_json::to_string(&schemars::schema_for!(NanosecondBound)).unwrap();
    assert!(schema.contains("9007199254740991") && schema.contains("unbounded"));
}

#[test]
fn a_time_span_splits_negative_lengths_by_euclidean_division() {
    let span = TimeSpan::from_total_nanoseconds(-1_500_000_000).unwrap();
    assert_eq!((span.seconds(), span.nanosecond().get()), (-2, 500_000_000));
    assert!(span.is_negative());
    assert_eq!(
        serde_json::to_value(span).unwrap(),
        json!({"seconds":-2,"nanosecond":500_000_000})
    );
    assert_eq!(
        serde_json::from_value::<TimeSpan>(json!({"seconds":90})).unwrap(),
        TimeSpan::new(90, SubsecondNanoseconds::ZERO).unwrap()
    );
    assert!(TimeSpan::ZERO < TimeSpan::from_total_nanoseconds(1).unwrap());
    assert!(span < TimeSpan::ZERO);
    for refused in [
        json!({"seconds":0,"nanosecond":1_000_000_000}),
        json!({"seconds":0,"nanosecond":-1}),
        json!({"seconds":0.5}),
        json!({"nanosecond":0}),
        json!({"seconds":0,"undeclared":true}),
    ] {
        assert!(
            serde_json::from_value::<TimeSpan>(refused.clone()).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn checked_authority_wire_and_instant_reject_unknown_fields() {
    let binding = serde_json::to_value(authority()).unwrap();
    let mut extra = binding.clone();
    extra["undeclared"] = json!(true);
    assert!(serde_json::from_value::<AuthorityBinding>(extra).is_err());
    let input = json!({"taiSecondsSince1970":0,"nanosecond":0,
        "uncertainty":{"kind":"bounded","nanoseconds":0},"authority":binding});
    assert!(serde_json::from_value::<TimeInstant>(input.clone()).is_ok());
    for path in ["", "/authority"] {
        let mut extra = input.clone();
        extra
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("undeclared".to_owned(), json!(true));
        assert!(
            serde_json::from_value::<TimeInstant>(extra).is_err(),
            "{path}"
        );
    }
}
