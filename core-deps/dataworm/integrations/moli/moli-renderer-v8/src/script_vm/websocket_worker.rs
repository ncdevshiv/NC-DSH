//! Worker-owned WebSocket identity translation and synthetic server input.
//!
//! Worker host/control record application lives in
//! `worker_host_bridge_body`; this module only owns the collision-free socket
//! identity mapping shared by those records and the synthetic WebSocket
//! ingress used by the surrounding runtime.

use moli_shared_worker::SharedWorkerInstanceId;

use super::ScriptVm;
use crate::types::DedicatedWorkerId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EncodedWorkerWebSocketSocketId(u64);

impl EncodedWorkerWebSocketSocketId {
    const DEDICATED_WORKER_TAG: u64 = 0b10 << 62;
    const SHARED_WORKER_TAG: u64 = 0b11 << 62;
    const OWNER_ID_MASK: u64 = 0x3fff_ffff;
    const SOCKET_ID_MASK: u64 = 0xffff_ffff;

    fn dedicated(worker_id: DedicatedWorkerId, socket_id: u64) -> Self {
        Self(
            Self::DEDICATED_WORKER_TAG
                | ((worker_id.as_u64() & Self::OWNER_ID_MASK) << 32)
                | (socket_id & Self::SOCKET_ID_MASK),
        )
    }

    fn shared(instance_id: SharedWorkerInstanceId, socket_id: u64) -> Self {
        Self(
            Self::SHARED_WORKER_TAG
                | ((instance_id.as_u64() & Self::OWNER_ID_MASK) << 32)
                | (socket_id & Self::SOCKET_ID_MASK),
        )
    }

    pub(super) fn as_u64(self) -> u64 {
        self.0
    }
}

impl ScriptVm {
    pub(super) fn worker_websocket_socket_id(
        worker_id: DedicatedWorkerId,
        socket_id: u64,
    ) -> EncodedWorkerWebSocketSocketId {
        EncodedWorkerWebSocketSocketId::dedicated(worker_id, socket_id)
    }

    pub(super) fn shared_worker_websocket_socket_id(
        instance_id: SharedWorkerInstanceId,
        socket_id: u64,
    ) -> EncodedWorkerWebSocketSocketId {
        EncodedWorkerWebSocketSocketId::shared(instance_id, socket_id)
    }

    pub(super) fn shared_worker_websocket_lifecycle_event(
        instance_id: SharedWorkerInstanceId,
        event: &crate::worker::WorkerWebSocketLifecycleEvent,
    ) -> crate::types::WebSocketLifecycleEvent {
        let socket_id = Self::shared_worker_websocket_socket_id(instance_id, event.socket_id());
        match event {
            crate::worker::WorkerWebSocketLifecycleEvent::Open {
                document_url, url, ..
            } => crate::types::WebSocketLifecycleEvent::open(
                socket_id.as_u64(),
                document_url.clone(),
                url.clone(),
            ),
            crate::worker::WorkerWebSocketLifecycleEvent::Error {
                document_url,
                url,
                error_text,
                ..
            } => crate::types::WebSocketLifecycleEvent::error(
                socket_id.as_u64(),
                document_url.clone(),
                url.clone(),
                error_text.clone(),
            ),
            crate::worker::WorkerWebSocketLifecycleEvent::Closing {
                document_url, url, ..
            } => crate::types::WebSocketLifecycleEvent::closing(
                socket_id.as_u64(),
                document_url.clone(),
                url.clone(),
            ),
            crate::worker::WorkerWebSocketLifecycleEvent::Close {
                document_url,
                url,
                code,
                reason,
                was_clean,
                ..
            } => crate::types::WebSocketLifecycleEvent::close(
                socket_id.as_u64(),
                document_url.clone(),
                url.clone(),
                *code,
                reason.clone(),
                *was_clean,
            ),
        }
    }

    pub(super) fn shared_worker_websocket_frame_event(
        instance_id: SharedWorkerInstanceId,
        event: &crate::worker::WorkerWebSocketFrameEvent,
    ) -> crate::types::WebSocketNetworkEvent {
        crate::types::WebSocketNetworkEvent::new(
            Self::shared_worker_websocket_socket_id(instance_id, event.socket_id).as_u64(),
            event.document_url.clone(),
            event.url.clone(),
            event.direction,
            event.opcode,
            event.payload_length,
        )
    }

    pub(super) fn worker_websocket_lifecycle_event(
        worker_id: DedicatedWorkerId,
        event: &crate::worker::WorkerWebSocketLifecycleEvent,
    ) -> crate::types::WebSocketLifecycleEvent {
        let socket_id = Self::worker_websocket_socket_id(worker_id, event.socket_id());
        match event {
            crate::worker::WorkerWebSocketLifecycleEvent::Open {
                document_url, url, ..
            } => crate::types::WebSocketLifecycleEvent::open(
                socket_id.as_u64(),
                document_url.clone(),
                url.clone(),
            ),
            crate::worker::WorkerWebSocketLifecycleEvent::Error {
                document_url,
                url,
                error_text,
                ..
            } => crate::types::WebSocketLifecycleEvent::error(
                socket_id.as_u64(),
                document_url.clone(),
                url.clone(),
                error_text.clone(),
            ),
            crate::worker::WorkerWebSocketLifecycleEvent::Closing {
                document_url, url, ..
            } => crate::types::WebSocketLifecycleEvent::closing(
                socket_id.as_u64(),
                document_url.clone(),
                url.clone(),
            ),
            crate::worker::WorkerWebSocketLifecycleEvent::Close {
                document_url,
                url,
                code,
                reason,
                was_clean,
                ..
            } => crate::types::WebSocketLifecycleEvent::close(
                socket_id.as_u64(),
                document_url.clone(),
                url.clone(),
                *code,
                reason.clone(),
                *was_clean,
            ),
        }
    }

    pub(super) fn worker_websocket_frame_event(
        worker_id: DedicatedWorkerId,
        event: &crate::worker::WorkerWebSocketFrameEvent,
    ) -> crate::types::WebSocketNetworkEvent {
        crate::types::WebSocketNetworkEvent::new(
            Self::worker_websocket_socket_id(worker_id, event.socket_id).as_u64(),
            event.document_url.clone(),
            event.url.clone(),
            event.direction,
            event.opcode,
            event.payload_length,
        )
    }

    pub(crate) fn receive_synthetic_websocket_text(&self, socket_id: u64, data: String) -> bool {
        self._context_host
            .borrow()
            .receive_synthetic_websocket_text(socket_id, data)
    }

    pub(crate) fn receive_synthetic_websocket_binary(&self, socket_id: u64, data: Vec<u8>) -> bool {
        self._context_host
            .borrow()
            .receive_synthetic_websocket_binary(socket_id, data)
    }

    pub(crate) fn close_synthetic_websocket_from_server(
        &self,
        socket_id: u64,
        code: Option<u16>,
        reason: String,
    ) -> bool {
        self._context_host
            .borrow()
            .close_synthetic_websocket_from_server(socket_id, code, reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_worker_websocket_socket_ids_keep_worker_kind_in_tag() {
        let local_socket_id = 0xfeed_beef;
        let dedicated =
            EncodedWorkerWebSocketSocketId::dedicated(DedicatedWorkerId::new(17), local_socket_id);
        let shared = EncodedWorkerWebSocketSocketId::shared(
            SharedWorkerInstanceId::from_u64(17),
            local_socket_id,
        );

        assert_eq!(dedicated.as_u64() >> 62, 0b10);
        assert_eq!(shared.as_u64() >> 62, 0b11);
        assert_ne!(
            dedicated, shared,
            "encoded worker websocket ids must not collide when dedicated and SharedWorker owner ids share the same numeric value"
        );
        assert_eq!(
            dedicated.as_u64() & EncodedWorkerWebSocketSocketId::SOCKET_ID_MASK,
            local_socket_id
        );
        assert_eq!(
            shared.as_u64() & EncodedWorkerWebSocketSocketId::SOCKET_ID_MASK,
            local_socket_id
        );
    }
}
