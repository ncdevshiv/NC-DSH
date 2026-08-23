use url::Url;

use super::*;

fn url(input: &str) -> Url {
    Url::parse(input).unwrap()
}

#[test]
fn first_party_key_uses_url_origin_and_site() {
    let key = MoliStorageKey::first_party_from_url(&url("https://example.test/a"), None);

    assert_eq!(key.origin(), "https://example.test");
    assert_eq!(key.top_level_site(), "https://example.test");
    assert_eq!(
        key.partition_relation(),
        StoragePartitionRelation::FirstParty
    );
    assert!(!key.is_third_party_partitioned());
    assert!(!key.has_cross_site_ancestor());
    assert_eq!(
        key.serialized_storage_key(),
        "storage-key:v1;origin=https://example.test;top-level-site=https://example.test"
    );
}

#[test]
fn partitioned_key_tracks_top_level_site() {
    let top_level_site = site_for_url(&url("https://app.other.test/page.html"));
    let key = MoliStorageKey::from_url_and_top_level_site(
        &url("https://cdn.example.test/worker.js"),
        top_level_site,
        None,
    );

    assert_eq!(key.origin(), "https://cdn.example.test");
    assert_eq!(key.top_level_site(), "https://other.test");
    assert_eq!(
        key.partition_relation(),
        StoragePartitionRelation::ThirdParty
    );
    assert!(key.is_third_party_partitioned());
    assert!(key.has_cross_site_ancestor());
    assert_eq!(
        key.serialized_storage_key(),
        "storage-key:v1;origin=https://cdn.example.test;top-level-site=https://other.test"
    );
}

#[test]
fn schemeful_site_uses_registrable_domain_for_partition_relation() {
    let key = MoliStorageKey::from_url_and_top_level_site(
        &url("https://cdn.example.com/worker.js"),
        site_for_url(&url("https://app.example.com/page.html")),
        None,
    );

    assert_eq!(key.origin(), "https://cdn.example.com");
    assert_eq!(key.top_level_site(), "https://example.com");
    assert_eq!(
        key.partition_relation(),
        StoragePartitionRelation::FirstParty
    );
    assert!(!key.is_third_party_partitioned());
    assert_eq!(
        key.serialized_storage_key(),
        "storage-key:v1;origin=https://cdn.example.com;top-level-site=https://example.com"
    );
}

#[test]
fn schemeful_site_keeps_public_suffix_siblings_partitioned() {
    let key = MoliStorageKey::from_url_and_top_level_site(
        &url("https://foo.github.io/worker.js"),
        site_for_url(&url("https://bar.github.io/page.html")),
        None,
    );

    assert_eq!(key.origin(), "https://foo.github.io");
    assert_eq!(key.top_level_site(), "https://bar.github.io");
    assert_eq!(
        key.partition_relation(),
        StoragePartitionRelation::ThirdParty
    );
    assert!(key.is_third_party_partitioned());
    assert_eq!(
        key.serialized_storage_key(),
        "storage-key:v1;origin=https://foo.github.io;top-level-site=https://bar.github.io"
    );
}

#[test]
fn unknown_partition_relation_is_not_reported_as_third_party() {
    let key = MoliStorageKey::new(
        "null".to_owned(),
        "https://example.test".to_owned(),
        Some(OpaqueOriginNonce::new(1)),
        StoragePartitionRelation::Unknown,
    );

    assert_eq!(key.partition_relation(), StoragePartitionRelation::Unknown);
    assert!(!key.is_third_party_partitioned());
    assert_eq!(
        key.serialized_storage_key(),
        "storage-key:v1;origin=null;top-level-site=https://example.test;opaque-nonce=1"
    );
    assert!(serialized_storage_key_has_opaque_origin(
        &key.serialized_storage_key()
    ));
}

#[test]
fn opaque_nonce_keeps_null_origins_distinct() {
    let left = MoliStorageKey::new(
        "null".to_owned(),
        "null".to_owned(),
        Some(OpaqueOriginNonce::new(1)),
        StoragePartitionRelation::Unknown,
    );
    let right = MoliStorageKey::new(
        "null".to_owned(),
        "null".to_owned(),
        Some(OpaqueOriginNonce::new(2)),
        StoragePartitionRelation::Unknown,
    );

    assert_ne!(left, right);
    assert_eq!(left.origin(), "null");
    assert!(serialized_storage_key_has_opaque_origin(
        &left.serialized_storage_key()
    ));
}

#[test]
fn serialized_storage_key_round_trips_first_party_and_partitioned_keys() {
    let first_party = MoliStorageKey::first_party_from_url(&url("https://example.test"), None);
    assert_eq!(
        deserialize_serialized_storage_key(&first_party.serialized_storage_key()),
        Some(first_party)
    );

    let partitioned = MoliStorageKey::from_url_and_top_level_site(
        &url("https://cdn.example/script.js"),
        "https://app.example".to_owned(),
        None,
    );
    assert_eq!(
        deserialize_serialized_storage_key(&partitioned.serialized_storage_key()),
        Some(partitioned)
    );
}

#[test]
fn cross_site_ancestor_distinguishes_nested_first_party_storage() {
    let first_party = MoliStorageKey::first_party_from_url(&url("https://example.test"), None);
    let nested = first_party.clone().with_cross_site_ancestor();

    assert_ne!(nested, first_party);
    assert!(!nested.is_third_party_partitioned());
    assert!(nested.has_cross_site_ancestor());
    assert_eq!(
        nested.serialized_storage_key(),
        "storage-key:v1;origin=https://example.test;top-level-site=https://example.test;cross-site-ancestor=1"
    );
    assert_eq!(
        deserialize_serialized_storage_key(&nested.serialized_storage_key()),
        Some(nested)
    );
}

#[test]
fn serialized_storage_key_parser_rejects_non_origin_urls() {
    assert!(deserialize_serialized_storage_key("https://example.test").is_none());
    assert!(deserialize_serialized_storage_key("https://example.test/path").is_none());
    assert!(deserialize_serialized_storage_key("not a storage key").is_none());
}

#[test]
fn serialized_storage_key_parser_rejects_noncanonical_components() {
    let prefix = "storage-key:v1;origin=https://example.test;top-level-site=https://example.test";
    assert!(
        deserialize_serialized_storage_key(&format!(
            "{prefix};opaque-nonce=1;cross-site-ancestor=1"
        ))
        .is_none()
    );
    assert!(
        deserialize_serialized_storage_key(&format!(
            "{prefix};cross-site-ancestor=1;cross-site-ancestor=1"
        ))
        .is_none()
    );
    assert!(
        deserialize_serialized_storage_key(&format!("{prefix};opaque-nonce=1;opaque-nonce=2"))
            .is_none()
    );
}

#[test]
fn data_urls_need_opaque_nonce() {
    assert!(url_needs_opaque_nonce(&url("data:text/html,hello")));
    assert!(!url_needs_opaque_nonce(&url("https://example.test/")));
}
