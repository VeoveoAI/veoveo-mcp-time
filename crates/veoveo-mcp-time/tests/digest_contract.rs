use serde_json::json;
use veoveo_mcp_time::{StartAcquisitionRequest, TimeSourceId};
use veoveo_types::Sha256Digest;

#[test]
fn acquisition_requests_carry_prefixed_lowercase_digests() {
    for digest in [None, Some(Sha256Digest::from_hex("ab".repeat(32)).unwrap())] {
        let request = veoveo_mcp_time::StartAcquisitionRequestValue {
            source_id: TimeSourceId::parse("time-source-fixture").unwrap(),
            expected_source_digest: digest.clone(),
            idempotency_key: "fixture".into(),
        }
        .build()
        .unwrap();
        let wire = serde_json::to_value(&request).unwrap();
        assert_eq!(
            wire["expectedSourceDigest"],
            json!(digest.as_ref().map(Sha256Digest::as_str))
        );
        assert_eq!(
            serde_json::from_value::<StartAcquisitionRequest>(wire).unwrap(),
            request
        );
    }
    let wire = json!({"sourceId": "time-source-fixture",
        "expectedSourceDigest": format!("sha256:{}", "ab".repeat(32)), "idempotencyKey": "fixture"});
    assert!(serde_json::from_value::<StartAcquisitionRequest>(wire).is_ok());
}

#[test]
fn malformed_digests_fail_at_request_decoding_without_echoing_input() {
    for input in [
        String::new(),
        "a".repeat(64),
        format!("sha256:{}", "A".repeat(64)),
        format!("sha256:{}", "a".repeat(63)),
        format!("sha256:{}", "a".repeat(65)),
        format!("sha256:{}", "g".repeat(64)),
        format!("sha256:{}\n", "a".repeat(64)),
        "PRIVATE_INVALID_DIGEST".into(),
    ] {
        let request = json!({"sourceId":"time-source-fixture",
            "expectedSourceDigest":input, "idempotencyKey":"fixture"});
        let error = serde_json::from_value::<StartAcquisitionRequest>(request).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_INVALID_DIGEST"));
    }
    for value in [json!(12), json!({}), json!([])] {
        let request = json!({"sourceId":"time-source-fixture",
            "expectedSourceDigest":value, "idempotencyKey":"fixture"});
        assert!(serde_json::from_value::<StartAcquisitionRequest>(request).is_err());
    }
}
