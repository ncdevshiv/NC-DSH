use crate::context_bootstrap::indexed_db::idb_dom_string_list_backing_values;

pub(in crate::context_bootstrap::indexed_db) fn dom_string_list_values<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    list: v8::Local<'s, v8::Object>,
) -> Vec<String> {
    let Some(values) = idb_dom_string_list_backing_values(scope, list) else {
        return Vec::new();
    };
    let mut result = Vec::with_capacity(values.length() as usize);
    for index in 0..values.length() {
        if let Some(value) = values.get_index(scope, index)
            && let Some(text) = value.to_string(scope)
        {
            result.push(text.to_rust_string_lossy(scope));
        }
    }
    result
}
