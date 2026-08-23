use std::sync::{Arc, OnceLock};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

// Chromium keeps both a renderer-side WebSocket count and a network-service
// pending-handshake throttle at 255 per process/profile. Moli has one
// renderer/runtime profile today, so these are process-wide caps.
pub(crate) const MAX_WEBSOCKET_CONNECTIONS_PER_RUNTIME: usize = 255;
pub(crate) const MAX_PENDING_WEBSOCKET_HANDSHAKES: usize = 255;

pub(crate) fn acquire_websocket_connection_slot() -> Option<OwnedSemaphorePermit> {
    acquire_limited_websocket_slot(websocket_connection_slots())
}

pub(crate) fn acquire_pending_websocket_handshake_slot() -> Option<OwnedSemaphorePermit> {
    acquire_limited_websocket_slot(pending_websocket_handshake_slots())
}

pub(crate) fn acquire_limited_websocket_slot(
    slots: &Arc<Semaphore>,
) -> Option<OwnedSemaphorePermit> {
    slots.clone().try_acquire_owned().ok()
}

fn websocket_connection_slots() -> &'static Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(MAX_WEBSOCKET_CONNECTIONS_PER_RUNTIME)))
}

fn pending_websocket_handshake_slots() -> &'static Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(MAX_PENDING_WEBSOCKET_HANDSHAKES)))
}
