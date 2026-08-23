use super::*;

pub(in crate::context_bootstrap::indexed_db) fn index_info_from_store_metadata<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    store: v8::Local<'s, v8::Object>,
    index_name: &str,
) -> Option<IndexInfo> {
    indexed_db_object_store_metadata(scope, store)?
        .index(index_name)
        .cloned()
}

pub(in crate::context_bootstrap::indexed_db) fn set_database_index_metadata<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    database: v8::Local<'s, v8::Object>,
    store_name: &str,
    info: &IndexInfo,
) -> Option<()> {
    set_indexed_db_database_index_metadata(scope, database, store_name, info.clone())
}

pub(in crate::context_bootstrap::indexed_db) fn remove_database_index_metadata<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    database: v8::Local<'s, v8::Object>,
    store_name: &str,
    index_name: &str,
) -> Option<()> {
    remove_indexed_db_database_index_metadata(scope, database, store_name, index_name)
}
