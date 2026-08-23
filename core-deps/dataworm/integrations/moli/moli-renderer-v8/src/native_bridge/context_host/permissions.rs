use serde_json::Value;

pub(super) fn permission_override_name(
    override_entry: &crate::protocol_types::PermissionOverrideRegistration,
) -> Option<&str> {
    match &override_entry.permission {
        Value::String(name) => Some(name.as_str()),
        Value::Object(map) => map.get("name").and_then(Value::as_str),
        _ => None,
    }
}

pub(super) fn permission_names_match(configured: &str, requested: &str) -> bool {
    configured == requested
        || matches!(
            (configured, requested),
            ("idleDetection", "idle-detection") | ("idle-detection", "idleDetection")
        )
}
