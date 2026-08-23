//! Renderer adapter for the renderer-neutral BroadcastChannel registry.
//!
//! `moli-broadcast-channel` owns id allocation, storage-key/name routing,
//! and pending queues. This module binds that service to renderer-specific
//! concerns: structured-clone payloads, page/worker wake handles, and the
//! shared `Arc` lifetime used by `RendererOwnerState`.

use std::sync::Arc;

pub(crate) use moli_broadcast_channel::BroadcastChannelEvent;
use moli_broadcast_channel::BroadcastChannelRegistry as CoreBroadcastChannelRegistry;
pub(crate) use moli_storage_key::MoliStorageKey as BroadcastChannelStorageKey;
use tokio::sync::mpsc;

use crate::{
    page_task_queue::RendererPageBroadcastChannelDeliveryProducer,
    structured_clone::V8StructuredClonePayload, types::BroadcastChannelId, worker::WorkerMessage,
};

#[derive(Clone, Debug)]
pub(crate) enum BroadcastChannelOwner {
    /// Page-owned delivery publishes an exact Window-realm task to the stable
    /// Page scheduler.
    Page(RendererPageBroadcastChannelDeliveryProducer),
    /// Worker-owned channel delivery wakes the worker event loop.
    Worker(mpsc::UnboundedSender<WorkerMessage>),
}

type RendererBroadcastChannelRegistry =
    CoreBroadcastChannelRegistry<V8StructuredClonePayload, BroadcastChannelOwner>;

/// Renderer-facing BroadcastChannel service.
///
/// The wrapper keeps renderer types out of the generic crate while presenting
/// the old local API to V8 binding code.
#[derive(Debug, Default)]
pub(crate) struct BroadcastChannelRegistry {
    inner: RendererBroadcastChannelRegistry,
}

/// Registry shared by one renderer owner and the workers it creates.
pub(crate) type SharedBroadcastChannelRegistry = Arc<BroadcastChannelRegistry>;

pub(crate) fn new_broadcast_channel_registry() -> SharedBroadcastChannelRegistry {
    Arc::new(BroadcastChannelRegistry::default())
}

impl BroadcastChannelRegistry {
    pub(crate) fn create_broadcast_channel(
        &self,
        storage_key: BroadcastChannelStorageKey,
        name: String,
        owner: BroadcastChannelOwner,
    ) -> BroadcastChannelId {
        self.inner
            .create_broadcast_channel(storage_key, name, owner)
    }

    pub(crate) fn next_opaque_context_nonce(&self) -> moli_storage_key::OpaqueOriginNonce {
        self.inner.next_opaque_context_nonce()
    }

    pub(crate) fn close_broadcast_channel(&self, channel_id: BroadcastChannelId) {
        self.inner.close_broadcast_channel(channel_id);
    }

    pub(crate) fn post_broadcast_channel_message(
        &self,
        source_id: BroadcastChannelId,
        payload: V8StructuredClonePayload,
    ) -> Vec<BroadcastChannelId> {
        self.inner
            .post_broadcast_channel_message(source_id, payload)
    }

    pub(crate) fn take_pending_broadcast_channel_event(
        &self,
        channel_id: BroadcastChannelId,
    ) -> Option<BroadcastChannelEvent<V8StructuredClonePayload>> {
        self.inner.take_pending_broadcast_channel_event(channel_id)
    }

    pub(crate) fn broadcast_channel_origin(
        &self,
        channel_id: BroadcastChannelId,
    ) -> Option<String> {
        self.inner.broadcast_channel_origin(channel_id)
    }

    pub(crate) fn wake_broadcast_channel_if_pending(&self, channel_id: BroadcastChannelId) -> bool {
        self.inner
            .wake_broadcast_channel_if_pending(channel_id, wake_owner)
    }
}

fn wake_owner(owner: BroadcastChannelOwner, channel_id: BroadcastChannelId) -> bool {
    match owner {
        BroadcastChannelOwner::Page(producer) => producer.send(channel_id).is_ok(),
        BroadcastChannelOwner::Worker(sender) => sender
            .send(WorkerMessage::BroadcastChannelWake(channel_id))
            .is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moli_storage_key::{OpaqueOriginNonce, StoragePartitionRelation};
    use url::Url;

    fn test_owner() -> BroadcastChannelOwner {
        let (sender, _receiver) = mpsc::unbounded_channel();
        BroadcastChannelOwner::Worker(sender)
    }

    fn first_party_key(origin: &str) -> BroadcastChannelStorageKey {
        let url = Url::parse(origin).unwrap();
        BroadcastChannelStorageKey::first_party_from_url(&url, None)
    }

    fn partitioned_key(origin: &str, top_level_url: &str) -> BroadcastChannelStorageKey {
        let url = Url::parse(origin).unwrap();
        let top_level_url = Url::parse(top_level_url).unwrap();
        BroadcastChannelStorageKey::from_url_and_top_level_site(
            &url,
            moli_storage_key::site_for_url(&top_level_url),
            None,
        )
    }

    fn opaque_key(nonce: u64) -> BroadcastChannelStorageKey {
        BroadcastChannelStorageKey::new(
            "null".to_owned(),
            "null".to_owned(),
            Some(OpaqueOriginNonce::new(nonce)),
            StoragePartitionRelation::Unknown,
        )
    }

    #[test]
    fn separate_registries_do_not_route_same_origin_and_name() {
        let left = new_broadcast_channel_registry();
        let right = new_broadcast_channel_registry();
        let source = left.create_broadcast_channel(
            first_party_key("https://example.test/"),
            "updates".to_owned(),
            test_owner(),
        );
        let target = right.create_broadcast_channel(
            first_party_key("https://example.test/"),
            "updates".to_owned(),
            test_owner(),
        );

        let recipients =
            left.post_broadcast_channel_message(source, V8StructuredClonePayload::default());

        assert!(recipients.is_empty());
        assert!(right.take_pending_broadcast_channel_event(target).is_none());
    }

    #[test]
    fn shared_registry_routes_by_storage_key_and_name() {
        let registry = new_broadcast_channel_registry();
        let source = registry.create_broadcast_channel(
            first_party_key("https://example.test/"),
            "updates".to_owned(),
            test_owner(),
        );
        let target = registry.create_broadcast_channel(
            first_party_key("https://example.test/"),
            "updates".to_owned(),
            test_owner(),
        );

        let recipients =
            registry.post_broadcast_channel_message(source, V8StructuredClonePayload::default());

        assert_eq!(recipients, vec![target]);
        assert!(matches!(
            registry.take_pending_broadcast_channel_event(target),
            Some(BroadcastChannelEvent::Message(_))
        ));
    }

    #[test]
    fn shared_registry_does_not_route_across_top_level_site_partitions() {
        let registry = new_broadcast_channel_registry();
        let source = registry.create_broadcast_channel(
            partitioned_key(
                "https://cdn.example.test/worker.js",
                "https://app.example.test",
            ),
            "updates".to_owned(),
            test_owner(),
        );
        let target = registry.create_broadcast_channel(
            partitioned_key(
                "https://cdn.example.test/worker.js",
                "https://app.other.test",
            ),
            "updates".to_owned(),
            test_owner(),
        );

        let recipients =
            registry.post_broadcast_channel_message(source, V8StructuredClonePayload::default());

        assert!(recipients.is_empty());
        assert!(
            registry
                .take_pending_broadcast_channel_event(target)
                .is_none()
        );
    }

    #[test]
    fn shared_registry_does_not_route_across_opaque_nonces() {
        let registry = new_broadcast_channel_registry();
        let source =
            registry.create_broadcast_channel(opaque_key(1), "updates".to_owned(), test_owner());
        let target =
            registry.create_broadcast_channel(opaque_key(2), "updates".to_owned(), test_owner());

        let recipients =
            registry.post_broadcast_channel_message(source, V8StructuredClonePayload::default());

        assert!(recipients.is_empty());
        assert!(
            registry
                .take_pending_broadcast_channel_event(target)
                .is_none()
        );
    }

    #[test]
    fn storage_key_marks_third_party_partition() {
        let key = partitioned_key(
            "https://cdn.example.test/worker.js",
            "https://app.other.test",
        );

        assert_eq!(key.origin(), "https://cdn.example.test");
        assert_eq!(key.top_level_site(), "https://other.test");
        assert_eq!(
            key.partition_relation(),
            StoragePartitionRelation::ThirdParty
        );
        assert!(key.is_third_party_partitioned());
    }

    #[test]
    fn closing_channel_in_one_registry_does_not_close_same_id_in_another() {
        let left = new_broadcast_channel_registry();
        let right = new_broadcast_channel_registry();
        let left_channel = left.create_broadcast_channel(
            first_party_key("https://example.test/"),
            "updates".to_owned(),
            test_owner(),
        );
        let right_channel = right.create_broadcast_channel(
            first_party_key("https://example.test/"),
            "updates".to_owned(),
            test_owner(),
        );
        assert_eq!(left_channel, right_channel);

        left.close_broadcast_channel(left_channel);

        assert_eq!(
            right.broadcast_channel_origin(right_channel).as_deref(),
            Some("https://example.test")
        );
    }
}
