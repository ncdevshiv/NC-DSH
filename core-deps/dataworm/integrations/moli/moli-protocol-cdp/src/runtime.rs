use serde_json::{Value, json};

/// Build a CDP Runtime.RemoteObject from a plain JSON value.
///
/// Protocol-neutral command results may carry ordinary JSON values before they
/// are shaped for CDP clients. Keep this wire mapping in the CDP crate so
/// callers do not duplicate CDP-specific `type`/`subtype` conventions.
pub fn remote_object_from_json_value(value: Value, object_id: Option<String>) -> Value {
    let mut remote = match value {
        Value::Null => json!({ "type": "object", "subtype": "null", "value": null }),
        Value::Bool(value) => json!({ "type": "boolean", "value": value }),
        Value::Number(value) => json!({ "type": "number", "value": value }),
        Value::String(value) => json!({ "type": "string", "value": value }),
        Value::Array(value) => json!({ "type": "object", "subtype": "array", "value": value }),
        Value::Object(value) => json!({ "type": "object", "value": value }),
    };
    if let Some(object_id) = object_id
        && let Some(remote) = remote.as_object_mut()
    {
        remote.insert("objectId".to_owned(), json!(object_id));
    }
    remote
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_object_from_json_value_maps_cdp_runtime_shape() {
        assert_eq!(
            remote_object_from_json_value(Value::Null, None),
            json!({ "type": "object", "subtype": "null", "value": null })
        );
        assert_eq!(
            remote_object_from_json_value(json!([1, 2]), Some("obj-1".to_owned())),
            json!({ "type": "object", "subtype": "array", "value": [1, 2], "objectId": "obj-1" })
        );
    }
}
