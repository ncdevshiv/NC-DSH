use crate::{types::MessagePortId, worker::WorkerMessage};

use super::host::RendererSharedWorkerHost;

impl RendererSharedWorkerHost {
    pub(super) fn connect(&self, worker_port_id: MessagePortId) -> bool {
        self.send_worker_message(WorkerMessage::SharedWorkerConnect(worker_port_id))
    }
}
