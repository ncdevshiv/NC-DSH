use super::*;

pub(in crate::context_bootstrap::indexed_db) fn enforce_object_store_unique_constraints<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    store: v8::Local<'s, v8::Object>,
    handle: TransactionHandle,
    store_name: &str,
    primary_key: &Key,
    value: v8::Local<'s, v8::Value>,
) -> std::result::Result<(), IndexedDbError> {
    let indexes = indexed_db_object_store_metadata(scope, store)
        .map(|metadata| metadata.indexes_in_name_order())
        .unwrap_or_default();
    for index in indexes {
        if !index.unique {
            continue;
        }

        let candidate_keys =
            extract_index_keys_from_value(scope, value, &index.key_path, index.multi_entry);
        if candidate_keys.is_empty() {
            continue;
        }

        let mut seen = BTreeSet::new();
        for key in &candidate_keys {
            if !seen.insert(key.clone()) {
                return Err(IndexedDbError::Constraint(format!(
                    "unique index `{}` already contains key",
                    index.name
                )));
            }
        }

        let existing = scan_index_entries(scope, handle, store_name, &index, None)?;
        if existing
            .into_iter()
            .any(|entry| entry.primary_key != *primary_key && seen.contains(&entry.index_key))
        {
            return Err(IndexedDbError::Constraint(format!(
                "unique index `{}` already contains key",
                index.name
            )));
        }
    }
    Ok(())
}
