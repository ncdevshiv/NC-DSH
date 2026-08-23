use std::{
    collections::{BTreeMap, VecDeque},
    sync::atomic::{AtomicU64, Ordering},
};

use moli_storage_key::{MoliStorageKey, OpaqueOriginNonce};
use parking_lot::Mutex;

/// Stable runtime id for one registered BroadcastChannel object.
///
/// The id is local to one `BroadcastChannelRegistry`. Different registries may
/// allocate the same numeric id without sharing delivery state.
pub type BroadcastChannelId = u64;

/// Pending event queued for a target BroadcastChannel.
///
/// `P` is supplied by the embedding layer. In `moli-renderer-v8`, it is
/// the structured-clone byte payload that will later be deserialized inside the
/// target V8 context.
#[derive(Clone, Debug)]
pub enum BroadcastChannelEvent<P> {
    /// A successful `postMessage` payload waiting for delivery.
    Message(P),
}

#[derive(Debug)]
struct BroadcastChannelState<P, O> {
    /// Storage partition key used for routing and origin reporting.
    storage_key: MoliStorageKey,
    /// User-provided BroadcastChannel name.
    name: String,
    /// Embedding-owned wake target, such as a page queue or worker sender.
    owner: O,
    /// FIFO events waiting for the target context to dispatch.
    pending_events: VecDeque<BroadcastChannelEvent<P>>,
}

/// Registry shared by related pages/workers for BroadcastChannel delivery.
///
/// The registry is deliberately an in-process service, not a JavaScript object
/// table. It routes messages to matching channels and records which embedding
/// owner must be woken when a pending event targets another context.
#[derive(Debug)]
pub struct BroadcastChannelRegistry<P, O> {
    channels: Mutex<BTreeMap<BroadcastChannelId, BroadcastChannelState<P, O>>>,
    next_channel_id: AtomicU64,
    next_opaque_context_nonce: AtomicU64,
}

impl<P, O> Default for BroadcastChannelRegistry<P, O> {
    fn default() -> Self {
        Self {
            channels: Mutex::default(),
            next_channel_id: AtomicU64::default(),
            next_opaque_context_nonce: AtomicU64::default(),
        }
    }
}

impl<P, O> BroadcastChannelRegistry<P, O> {
    fn next_broadcast_channel_id(&self) -> BroadcastChannelId {
        self.next_channel_id
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    }

    /// Register a BroadcastChannel and return its runtime id.
    ///
    /// Routing later matches channels in the same registry with the same
    /// storage key and name, excluding the source channel itself.
    pub fn create_broadcast_channel(
        &self,
        storage_key: MoliStorageKey,
        name: String,
        owner: O,
    ) -> BroadcastChannelId {
        let channel_id = self.next_broadcast_channel_id();
        self.channels.lock().insert(
            channel_id,
            BroadcastChannelState {
                storage_key,
                name,
                owner,
                pending_events: VecDeque::new(),
            },
        );
        channel_id
    }

    /// Allocate a nonce for one opaque-origin context in this registry.
    ///
    /// Keeping the allocator on the registry makes opaque-origin identity
    /// unique across all pages/workers that can otherwise share channels.
    pub fn next_opaque_context_nonce(&self) -> OpaqueOriginNonce {
        OpaqueOriginNonce::new(
            self.next_opaque_context_nonce
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1),
        )
    }

    /// Remove a channel and drop any pending events for it.
    pub fn close_broadcast_channel(&self, channel_id: BroadcastChannelId) {
        self.channels.lock().remove(&channel_id);
    }

    /// Pop one pending event for a target channel.
    ///
    /// The embedding layer calls this after it has entered the target context
    /// and can turn the payload into a JavaScript `MessageEvent`.
    pub fn take_pending_broadcast_channel_event(
        &self,
        channel_id: BroadcastChannelId,
    ) -> Option<BroadcastChannelEvent<P>> {
        self.channels
            .lock()
            .get_mut(&channel_id)?
            .pending_events
            .pop_front()
    }

    /// Return the JavaScript-facing origin string for a channel.
    ///
    /// For opaque origins this intentionally returns `"null"`; the hidden
    /// nonce stays inside `MoliStorageKey` and is not exposed to script.
    pub fn broadcast_channel_origin(&self, channel_id: BroadcastChannelId) -> Option<String> {
        self.channels
            .lock()
            .get(&channel_id)
            .map(|channel| channel.storage_key.origin().to_owned())
    }

    /// Wake a channel owner if the channel still has pending work.
    ///
    /// The caller supplies the actual wake operation because this crate does
    /// not know how pages or workers are scheduled. If waking fails, the
    /// channel is removed because its owner is no longer reachable.
    pub fn wake_broadcast_channel_if_pending<F>(
        &self,
        channel_id: BroadcastChannelId,
        wake_owner: F,
    ) -> bool
    where
        O: Clone,
        F: FnOnce(O, BroadcastChannelId) -> bool,
    {
        let owner = {
            let channels = self.channels.lock();
            let Some(channel) = channels.get(&channel_id) else {
                return false;
            };
            if channel.pending_events.is_empty() {
                return true;
            }
            channel.owner.clone()
        };
        if wake_owner(owner, channel_id) {
            return true;
        }
        self.channels.lock().remove(&channel_id);
        false
    }
}

impl<P, O> BroadcastChannelRegistry<P, O>
where
    P: Clone,
{
    /// Queue a cloned payload for every matching target channel.
    ///
    /// Matching is `same registry + same storage key + same channel name`,
    /// excluding the source id. The returned ids tell the embedding layer which
    /// targets need a local callback or cross-context wake.
    pub fn post_broadcast_channel_message(
        &self,
        source_id: BroadcastChannelId,
        payload: P,
    ) -> Vec<BroadcastChannelId> {
        let mut channels = self.channels.lock();
        let Some(source) = channels.get(&source_id) else {
            return Vec::new();
        };
        let storage_key = source.storage_key.clone();
        let name = source.name.clone();
        channels
            .iter_mut()
            .filter_map(|(channel_id, channel)| {
                (*channel_id != source_id
                    && channel.storage_key == storage_key
                    && channel.name == name)
                    .then_some((*channel_id, channel))
            })
            .map(|(channel_id, channel)| {
                channel
                    .pending_events
                    .push_back(BroadcastChannelEvent::Message(payload.clone()));
                channel_id
            })
            .collect()
    }
}
