use crate::{
    message_port_runtime::SharedMessagePortRegistry,
    page_task_queue::{
        RendererPageSharedWorkerClientEventProducer, RendererWorkerHostBridgeEventSender,
    },
    types::MessagePortId,
    worker::WorkerParentErrorEventKind,
};

/// Endpoint ownership after one SharedWorker client error is consumed.
///
/// Runtime script errors keep the connected wrapper available for later
/// reports. Connection/load failures retire it before arbitrary Page JS can
/// reenter the SharedWorker registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SharedWorkerClientEndpointDisposition {
    Retain,
    Retire,
}

/// One user-visible error reported to an owning `SharedWorker` wrapper.
///
/// Keeping this payload separate from `Closed` makes it impossible for the V8
/// error dispatcher to receive a state-only endpoint event.
#[derive(Clone, Debug)]
pub(crate) struct SharedWorkerClientError {
    message: String,
    filename: String,
    lineno: u32,
    colno: u32,
    event_kind: WorkerParentErrorEventKind,
    endpoint_disposition: SharedWorkerClientEndpointDisposition,
}

impl SharedWorkerClientError {
    pub(crate) fn new(
        message: String,
        filename: String,
        lineno: u32,
        colno: u32,
        event_kind: WorkerParentErrorEventKind,
        endpoint_disposition: SharedWorkerClientEndpointDisposition,
    ) -> Self {
        Self {
            message,
            filename,
            lineno,
            colno,
            event_kind,
            endpoint_disposition,
        }
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn filename(&self) -> &str {
        &self.filename
    }

    pub(crate) const fn lineno(&self) -> u32 {
        self.lineno
    }

    pub(crate) const fn colno(&self) -> u32 {
        self.colno
    }

    pub(crate) const fn event_kind(&self) -> WorkerParentErrorEventKind {
        self.event_kind
    }

    pub(crate) const fn endpoint_disposition(&self) -> SharedWorkerClientEndpointDisposition {
        self.endpoint_disposition
    }
}

#[derive(Clone, Debug)]
pub(crate) enum SharedWorkerClientEvent {
    /// The browser-context runtime has already retired this client. The Page
    /// endpoint must forget its wrapper without calling remove-client again.
    Closed,
    /// A load/runtime error reported to the owning `SharedWorker` wrapper.
    /// Runtime errors can keep the endpoint alive; terminal connection errors
    /// retire it before listener reentrancy.
    Error(SharedWorkerClientError),
}

#[derive(Clone)]
pub(super) struct RendererSharedWorkerClient {
    pub(super) client_port_id: MessagePortId,
    pub(super) worker_port_id: MessagePortId,
    pub(super) message_port_registry: SharedMessagePortRegistry,
    pub(super) client_event_producer: RendererPageSharedWorkerClientEventProducer,
    pub(super) worker_host_bridge_sender: RendererWorkerHostBridgeEventSender,
}

impl RendererSharedWorkerClient {
    pub(super) fn close_ports(&self) {
        self.message_port_registry
            .close_message_port(self.client_port_id);
        self.message_port_registry
            .close_message_port(self.worker_port_id);
    }

    pub(super) fn close_worker_port(&self) {
        self.message_port_registry
            .close_message_port(self.worker_port_id);
    }

    pub(super) fn worker_port_id(&self) -> MessagePortId {
        self.worker_port_id
    }

    pub(super) fn worker_host_bridge_sender(&self) -> RendererWorkerHostBridgeEventSender {
        self.worker_host_bridge_sender.clone()
    }

    pub(super) fn send_error(
        &self,
        message: impl Into<String>,
        filename: impl Into<String>,
        event_kind: WorkerParentErrorEventKind,
    ) {
        self.send_error_with_location(message, filename, 0, 0, event_kind);
    }

    pub(super) fn send_error_with_location(
        &self,
        message: impl Into<String>,
        filename: impl Into<String>,
        lineno: u32,
        colno: u32,
        event_kind: WorkerParentErrorEventKind,
    ) {
        self.send_error_with_location_and_disposition(
            message,
            filename,
            lineno,
            colno,
            event_kind,
            SharedWorkerClientEndpointDisposition::Retire,
        );
    }

    pub(super) fn send_nonterminal_error_with_location(
        &self,
        message: impl Into<String>,
        filename: impl Into<String>,
        lineno: u32,
        colno: u32,
        event_kind: WorkerParentErrorEventKind,
    ) {
        self.send_error_with_location_and_disposition(
            message,
            filename,
            lineno,
            colno,
            event_kind,
            SharedWorkerClientEndpointDisposition::Retain,
        );
    }

    fn send_error_with_location_and_disposition(
        &self,
        message: impl Into<String>,
        filename: impl Into<String>,
        lineno: u32,
        colno: u32,
        event_kind: WorkerParentErrorEventKind,
        endpoint_disposition: SharedWorkerClientEndpointDisposition,
    ) {
        let _ = self
            .client_event_producer
            .send(SharedWorkerClientEvent::Error(
                SharedWorkerClientError::new(
                    message.into(),
                    filename.into(),
                    lineno,
                    colno,
                    event_kind,
                    endpoint_disposition,
                ),
            ));
    }

    pub(super) fn send_closed(&self) {
        let _ = self
            .client_event_producer
            .send(SharedWorkerClientEvent::Closed);
    }
}
