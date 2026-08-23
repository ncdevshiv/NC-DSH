//! Storage partition key primitives shared by Moli subsystems.
//!
//! This crate is the small, renderer-neutral piece of Moli's current
//! StorageKey model. It intentionally has no V8, page, worker, registry, or
//! network-loader state. Callers compute a `MoliStorageKey` at the API
//! boundary and pass it to services that need browser-like storage or
//! messaging isolation.
//!
//! The current key is a pragmatic subset of Chromium's StorageKey. It covers
//! the pieces Moli needs today:
//!
//! - the serialized origin exposed to JavaScript-facing APIs;
//! - a top-level site partition component;
//! - an internal nonce for opaque-origin realms whose serialized origin is
//!   always `"null"`;
//! - the known relation between the current site and the top-level site.
//! - whether the embedding ancestor chain contains a cross-site frame.
//!
//! This is not yet a complete Chromium `StorageKey` implementation, but its
//! site component follows Chromium-style schemeful registrable-site semantics.

mod key;
mod nonce;
mod relation;
mod serialization;
mod site;

pub use key::MoliStorageKey;
pub use nonce::OpaqueOriginNonce;
pub use relation::StoragePartitionRelation;
pub use serialization::{
    deserialize_serialized_storage_key, partitioned_storage_key,
    serialized_storage_key_has_opaque_origin, storage_key_for_origin_and_top_level_site,
    storage_key_prefix_for_origin,
};
pub use site::{MoliSite, site_for_url, url_needs_opaque_nonce};

#[cfg(test)]
mod tests;
