use parking_lot::Mutex;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ServiceWorkerRuntimeOwnerWake {
    ServiceLane,
}

#[derive(Debug, Clone)]
pub(crate) struct ServiceWorkerRuntimeOwnerWakeSender {
    tx: mpsc::UnboundedSender<ServiceWorkerRuntimeOwnerWake>,
}

impl ServiceWorkerRuntimeOwnerWakeSender {
    fn new(tx: mpsc::UnboundedSender<ServiceWorkerRuntimeOwnerWake>) -> Self {
        Self { tx }
    }

    pub(super) fn signal(&self, wake: ServiceWorkerRuntimeOwnerWake) -> bool {
        self.tx.send(wake).is_ok()
    }
}

#[derive(Default)]
pub(super) struct ServiceWorkerOwnerWake {
    owner_wake_txs: Mutex<Vec<ServiceWorkerRuntimeOwnerWakeSender>>,
}

impl ServiceWorkerOwnerWake {
    pub(super) fn add_owner_wake_sender(&self, sender: ServiceWorkerRuntimeOwnerWakeSender) {
        self.owner_wake_txs.lock().push(sender);
    }

    pub(super) fn signal_service_lane_wake(&self) -> bool {
        let mut senders = self.owner_wake_txs.lock();
        let before = senders.len();
        senders.retain(|sender| sender.signal(ServiceWorkerRuntimeOwnerWake::ServiceLane));
        before > 0 && !senders.is_empty()
    }
}

pub(crate) fn service_worker_owner_wake_channel() -> (
    ServiceWorkerRuntimeOwnerWakeSender,
    mpsc::UnboundedReceiver<ServiceWorkerRuntimeOwnerWake>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    (ServiceWorkerRuntimeOwnerWakeSender::new(tx), rx)
}
