use super::*;

pub(in crate::context_bootstrap::indexed_db) fn index_info_from_index_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    index: v8::Local<'s, v8::Object>,
) -> Option<IndexInfo> {
    indexed_db_index_info(scope, index)
}
