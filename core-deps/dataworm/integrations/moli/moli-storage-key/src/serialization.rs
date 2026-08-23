use crate::{
    MoliStorageKey, OpaqueOriginNonce, StoragePartitionRelation, site::site_for_serialized_origin,
};

const STORAGE_KEY_SERIALIZATION_PREFIX: &str = "storage-key:v1;origin=";

/// Parse Moli's current serialized storage-key shape.
///
/// This intentionally accepts only the serialization emitted by this crate.
/// Chromium's `StorageKey::Serialize()` has a different external syntax;
/// callers that expose CDP-compatible APIs should reject unsupported syntax
/// rather than collapsing it to an origin.
pub fn deserialize_serialized_storage_key(value: &str) -> Option<MoliStorageKey> {
    if let Some(rest) = value.strip_prefix(STORAGE_KEY_SERIALIZATION_PREFIX) {
        let (origin, rest) = rest.split_once(";top-level-site=")?;
        let mut components = rest.split(';').peekable();
        let top_level_site = components.next()?;
        let mut opaque_nonce = None;
        let mut cross_site_ancestor = false;
        if components
            .next_if(|component| *component == "cross-site-ancestor=1")
            .is_some()
        {
            cross_site_ancestor = true;
        }
        if let Some(component) = components.next() {
            let nonce = component.strip_prefix("opaque-nonce=")?;
            opaque_nonce = Some(OpaqueOriginNonce::new(nonce.parse::<u64>().ok()?));
        }
        if components.next().is_some() {
            return None;
        }
        let relation = StoragePartitionRelation::from_sites(
            &site_for_serialized_origin(origin),
            top_level_site,
        );
        let mut storage_key = MoliStorageKey::new(
            origin.to_owned(),
            top_level_site.to_owned(),
            opaque_nonce,
            relation,
        );
        if cross_site_ancestor {
            storage_key = storage_key.with_cross_site_ancestor();
        }
        return Some(storage_key);
    }

    None
}

/// Return the internal storage key for a non-opaque origin/top-level-site pair.
pub fn storage_key_for_origin_and_top_level_site(origin: &str, top_level_site: &str) -> String {
    format!("{STORAGE_KEY_SERIALIZATION_PREFIX}{origin};top-level-site={top_level_site}")
}

/// Return the prefix shared by all serialized storage keys for an origin.
pub fn storage_key_prefix_for_origin(origin: &str) -> String {
    format!("{STORAGE_KEY_SERIALIZATION_PREFIX}{origin};top-level-site=")
}

/// Return the internal storage key for a non-opaque third-party partition.
pub fn partitioned_storage_key(origin: &str, top_level_site: &str) -> String {
    storage_key_for_origin_and_top_level_site(origin, top_level_site)
}

/// Return whether a serialized storage key publicly represents an opaque origin.
pub fn serialized_storage_key_has_opaque_origin(storage_key: &str) -> bool {
    storage_key == "null"
        || storage_key
            .strip_prefix(STORAGE_KEY_SERIALIZATION_PREFIX)
            .is_some_and(|rest| rest == "null" || rest.starts_with("null;"))
}
