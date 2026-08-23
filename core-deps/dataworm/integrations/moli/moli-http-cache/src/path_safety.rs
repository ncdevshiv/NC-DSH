use std::path::{Component, Path};

pub(crate) fn safe_body_file_name(body_file: &str) -> bool {
    if !body_file.starts_with("body.") || !body_file.ends_with(".bin") || body_file.contains('\\') {
        return false;
    }
    let mut components = Path::new(body_file).components();
    // Body metadata is untrusted cache data, so accept exactly one relative
    // filename component and reject parent/root/prefix traversal on every OS.
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

pub(crate) fn safe_entry_key(key: &str) -> bool {
    // Public APIs accept a key string, but the on-disk layout needs it to be a
    // single predictable filename stem. Generated keys are lowercase hex.
    !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
