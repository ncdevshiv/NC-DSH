use super::*;

pub(super) fn index_info_for_collection<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    index: v8::Local<'s, v8::Object>,
    request: v8::Local<'s, v8::Object>,
) -> Option<IndexInfo> {
    index_info_from_index_object(scope, index).or_else(|| {
        let error =
            dom_exception_value(scope, "The requested index was not found.", "NotFoundError");
        store_request_error(scope, request, error);
        None
    })
}
