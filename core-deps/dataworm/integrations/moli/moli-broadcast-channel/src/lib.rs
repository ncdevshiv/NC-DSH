//! Renderer-neutral BroadcastChannel routing service.
//!
//! This crate owns the part of BroadcastChannel that does not need to know
//! about V8, JavaScript objects, pages, workers, or task queues:
//!
//! - channel id allocation;
//! - routing by `MoliStorageKey + channel name`;
//! - per-channel pending event queues;
//! - channel unregister/close behavior;
//! - opaque-origin nonce allocation for contexts that share this registry.
//!
//! The payload and owner types are generic. The renderer crate supplies
//! structured-clone bytes as the payload and page/worker wake handles as the
//! owner. That keeps this crate reusable while preserving the existing renderer
//! wrapper model: this registry never stores or dispatches V8 objects.

mod registry;

pub use registry::{BroadcastChannelEvent, BroadcastChannelId, BroadcastChannelRegistry};

#[cfg(test)]
mod tests;
