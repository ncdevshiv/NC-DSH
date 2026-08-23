use super::*;
use moli_storage_key::{MoliStorageKey, OpaqueOriginNonce, StoragePartitionRelation};

fn first_party_key(origin: &str) -> MoliStorageKey {
    let url = url::Url::parse(origin).unwrap();
    MoliStorageKey::first_party_from_url(&url, None)
}

fn partitioned_key(origin: &str, top_level_url: &str) -> MoliStorageKey {
    let url = url::Url::parse(origin).unwrap();
    let top_level_url = url::Url::parse(top_level_url).unwrap();
    MoliStorageKey::from_url_and_top_level_site(
        &url,
        moli_storage_key::site_for_url(&top_level_url),
        None,
    )
}

fn opaque_key(nonce: u64) -> MoliStorageKey {
    MoliStorageKey::new(
        "null".to_owned(),
        "null".to_owned(),
        Some(OpaqueOriginNonce::new(nonce)),
        StoragePartitionRelation::Unknown,
    )
}

#[test]
fn routes_by_storage_key_and_name() {
    let registry = BroadcastChannelRegistry::<Vec<u8>, u8>::default();
    let source =
        registry.create_broadcast_channel(first_party_key("https://example.test/"), "n".into(), 1);
    let target =
        registry.create_broadcast_channel(first_party_key("https://example.test/"), "n".into(), 2);

    let recipients = registry.post_broadcast_channel_message(source, b"hello".to_vec());

    assert_eq!(recipients, vec![target]);
    assert!(matches!(
        registry.take_pending_broadcast_channel_event(target),
        Some(BroadcastChannelEvent::Message(payload)) if payload == b"hello"
    ));
}

#[test]
fn does_not_route_across_top_level_site_partitions() {
    let registry = BroadcastChannelRegistry::<(), u8>::default();
    let source = registry.create_broadcast_channel(
        partitioned_key(
            "https://cdn.example.test/worker.js",
            "https://app.example.test",
        ),
        "n".into(),
        1,
    );
    let target = registry.create_broadcast_channel(
        partitioned_key(
            "https://cdn.example.test/worker.js",
            "https://app.other.test",
        ),
        "n".into(),
        2,
    );

    let recipients = registry.post_broadcast_channel_message(source, ());

    assert!(recipients.is_empty());
    assert!(
        registry
            .take_pending_broadcast_channel_event(target)
            .is_none()
    );
}

#[test]
fn does_not_route_across_opaque_nonces() {
    let registry = BroadcastChannelRegistry::<(), u8>::default();
    let source = registry.create_broadcast_channel(opaque_key(1), "n".into(), 1);
    let target = registry.create_broadcast_channel(opaque_key(2), "n".into(), 2);

    let recipients = registry.post_broadcast_channel_message(source, ());

    assert!(recipients.is_empty());
    assert!(
        registry
            .take_pending_broadcast_channel_event(target)
            .is_none()
    );
}

#[test]
fn wake_removes_channel_when_owner_cannot_be_woken() {
    let registry = BroadcastChannelRegistry::<(), u8>::default();
    let source =
        registry.create_broadcast_channel(first_party_key("https://example.test/"), "n".into(), 1);
    let target =
        registry.create_broadcast_channel(first_party_key("https://example.test/"), "n".into(), 2);
    assert_eq!(
        registry.post_broadcast_channel_message(source, ()),
        vec![target]
    );

    assert!(!registry.wake_broadcast_channel_if_pending(target, |_owner, _id| false));
    assert_eq!(registry.broadcast_channel_origin(target), None);
}
