use super::*;
use crate::util::serialize_v8_iter_array;

pub(in crate::context_bootstrap::indexed_db) fn execute_object_store_get_all_request<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    request: v8::Local<'s, v8::Object>,
    handle: TransactionHandle,
    store_name: &str,
    query_value: v8::Local<'s, v8::Value>,
    count_value: v8::Local<'s, v8::Value>,
    direction: CursorDirection,
) {
    let Some((query, count)) = collection_parse::parse_collection_query_and_count(
        scope,
        request,
        query_value,
        count_value,
        "getAll",
    ) else {
        return;
    };
    match scan_object_store_entries(scope, handle, store_name, query.as_ref()) {
        Ok(entries) => {
            let entries = apply_object_store_collection_direction(entries, direction);
            let limit = count.unwrap_or(entries.len());
            let values = entries
                .iter()
                .take(limit)
                .map(|(_, bytes)| {
                    deserialize_js_value(scope, bytes)
                        .unwrap_or_else(|| v8::undefined(scope).into())
                })
                .collect::<Vec<_>>();
            let array =
                serialize_v8_iter_array(scope, values).unwrap_or_else(|| v8::Array::new(scope, 0));
            store_request_success(scope, request, array.into());
        }
        Err(error) => {
            let error = request_error_object(scope, &error);
            store_request_error(scope, request, error);
        }
    }
}
