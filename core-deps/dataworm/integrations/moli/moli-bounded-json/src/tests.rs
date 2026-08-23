use serde::{
    Serialize,
    ser::{Error as _, Serializer},
};
use serde_json::json;

use super::*;

#[test]
fn exact_limit_succeeds() {
    let value = json!({ "value": "bounded" });
    let expected = serde_json::to_string(&value).expect("serialize fixture");

    assert_eq!(
        to_string_with_limit(&value, expected.len()).expect("serialize at exact limit"),
        expected
    );
}

#[test]
fn json_string_between_reuses_capacity_and_matches_serde_json_escaping() {
    let mut value = String::with_capacity(256);
    value.push_str("quotes=\" slash=\\ controls=\0\u{0001}\u{0008}\t\n\u{000c}\r unicode=中文");
    let original_pointer = value.as_ptr();
    let expected = format!(
        "{{\"value\":{}}}",
        serde_json::to_string(&value).expect("string JSON")
    );

    let output = json_string_between_with_limit(value, "{\"value\":\"", "\"}", expected.len())
        .expect("in-place JSON string at exact limit");

    assert_eq!(output, expected);
    assert_eq!(output.as_ptr(), original_pointer);
}

#[test]
fn json_string_between_rejects_output_over_limit() {
    assert!(matches!(
        json_string_between_with_limit("\"".to_owned(), "[\"", "\"]", 5),
        Err(BoundedJsonError::LimitExceeded { limit: 5 })
    ));
}

#[test]
fn oversized_output_stops_at_limit() {
    let value = json!({ "value": "too large" });
    let limit = 4;

    assert!(matches!(
        to_string_with_limit(&value, limit),
        Err(BoundedJsonError::LimitExceeded {
            limit: actual_limit
        }) if actual_limit == limit
    ));
}

#[test]
fn serialization_errors_remain_distinct_from_size_errors() {
    struct FailingValue;

    impl Serialize for FailingValue {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(S::Error::custom("fixture serialization failure"))
        }
    }

    assert!(matches!(
        to_string_with_limit(&FailingValue, 1024),
        Err(BoundedJsonError::Serialization(error))
            if error.to_string().contains("fixture serialization failure")
    ));
}
