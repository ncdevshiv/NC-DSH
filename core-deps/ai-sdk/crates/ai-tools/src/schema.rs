//! JSON-Schema argument validation for tools.

use serde_json::Value;

/// A validation failure with a human-readable reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentError(pub String);

impl std::fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validates `arguments` against a JSON Schema (object type with
/// properties/required/enum/type constraints).
///
/// Supports the subset of JSON Schema used by tool definitions: `type`,
/// `properties`, `required`, `enum`, `items` (arrays). Unknown keywords are
/// ignored. Returns [`ArgumentError`] on the first failure.
pub fn validate_arguments(schema: &Value, arguments: &Value) -> Result<(), ArgumentError> {
    // Schema must be an object; if not, treat as unconstrained.
    let Some(schema) = schema.as_object() else {
        return Ok(());
    };

    // Root type check.
    if let Some(expected) = schema.get("type").and_then(|t| t.as_str()) {
        if !value_matches_type(expected, arguments) {
            return Err(ArgumentError(format!(
                "expected {expected}, got {}",
                value_type_name(arguments)
            )));
        }
    }

    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        if let Some(obj) = arguments.as_object() {
            for (name, prop_schema) in properties {
                if let Some(value) = obj.get(name) {
                    validate_property(name, prop_schema, value)?;
                }
            }
        }
    }

    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for requirement in required {
            let name = requirement
                .as_str()
                .ok_or_else(|| ArgumentError("required entry is not a string".to_string()))?;
            let present = match arguments {
                Value::Object(map) => map.contains_key(name),
                _ => false,
            };
            if !present {
                return Err(ArgumentError(format!("missing required argument `{name}`")));
            }
        }
    }

    if let Some(allowed) = schema.get("enum").and_then(|e| e.as_array()) {
        if !allowed.contains(arguments) {
            return Err(ArgumentError(format!(
                "value not in allowed set: {arguments}"
            )));
        }
    }

    Ok(())
}

fn validate_property(name: &str, prop_schema: &Value, value: &Value) -> Result<(), ArgumentError> {
    if let Some(expected) = prop_schema.get("type").and_then(|t| t.as_str()) {
        if !value_matches_type(expected, value) {
            return Err(ArgumentError(format!(
                "argument `{name}`: expected {expected}, got {}",
                value_type_name(value)
            )));
        }
    }
    if let Some(allowed) = prop_schema.get("enum").and_then(|e| e.as_array()) {
        if !allowed.contains(value) {
            return Err(ArgumentError(format!(
                "argument `{name}`: value not in allowed set"
            )));
        }
    }
    if let Some(items) = prop_schema.get("items") {
        if let Some(array) = value.as_array() {
            for item in array {
                validate_property(name, items, item)?;
            }
        }
    }
    Ok(())
}

fn value_matches_type(expected: &str, value: &Value) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true, // unknown types are not enforced
    }
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_required_and_types() {
        let schema = json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer"}
            },
            "required": ["query"]
        });
        assert!(validate_arguments(&schema, &json!({"query": "rust"})).is_ok());
        assert!(validate_arguments(&schema, &json!({"query": "rust", "limit": 5})).is_ok());
        assert!(validate_arguments(&schema, &json!({})).is_err());
        assert!(validate_arguments(&schema, &json!({"query": 42})).is_err());
        assert!(validate_arguments(&schema, &json!({"query": "x", "limit": "five"})).is_err());
    }

    #[test]
    fn validates_enums_and_arrays() {
        let schema = json!({
            "type": "object",
            "properties": {
                "format": {"type": "string", "enum": ["rfc3339", "unix"]},
                "tags": {"type": "array", "items": {"type": "string"}}
            }
        });
        assert!(validate_arguments(&schema, &json!({"format": "unix", "tags": ["a"]})).is_ok());
        assert!(validate_arguments(&schema, &json!({"format": "other"})).is_err());
        assert!(validate_arguments(&schema, &json!({"tags": [1]})).is_err());
    }

    #[test]
    fn root_type_enforced() {
        let schema = json!({"type": "object"});
        assert!(validate_arguments(&schema, &json!({"a": 1})).is_ok());
        assert!(validate_arguments(&schema, &json!([1, 2])).is_err());
    }
}
