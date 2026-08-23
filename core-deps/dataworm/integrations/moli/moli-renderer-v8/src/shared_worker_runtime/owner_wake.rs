use parking_lot::Mutex;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SharedWorkerRuntimeOwnerWake {
    ServiceLane,
}

#[derive(Debug, Clone)]
pub(crate) struct SharedWorkerRuntimeOwnerWakeSender {
    tx: mpsc::UnboundedSender<SharedWorkerRuntimeOwnerWake>,
}

impl SharedWorkerRuntimeOwnerWakeSender {
    fn new(tx: mpsc::UnboundedSender<SharedWorkerRuntimeOwnerWake>) -> Self {
        Self { tx }
    }

    pub(super) fn signal(&self, wake: SharedWorkerRuntimeOwnerWake) -> bool {
        self.tx.send(wake).is_ok()
    }
}

#[derive(Default)]
pub(super) struct SharedWorkerOwnerWake {
    owner_wake_txs: Mutex<Vec<SharedWorkerRuntimeOwnerWakeSender>>,
}

impl SharedWorkerOwnerWake {
    pub(super) fn add_owner_wake_sender(&self, sender: SharedWorkerRuntimeOwnerWakeSender) {
        self.owner_wake_txs.lock().push(sender);
    }

    pub(super) fn signal_service_lane_wake(&self) -> bool {
        self.signal_owner_wake(SharedWorkerRuntimeOwnerWake::ServiceLane)
    }

    fn signal_owner_wake(&self, wake: SharedWorkerRuntimeOwnerWake) -> bool {
        let mut senders = self.owner_wake_txs.lock();
        let before = senders.len();
        senders.retain(|sender| sender.signal(wake));
        before > 0 && !senders.is_empty()
    }
}

pub(crate) fn shared_worker_owner_wake_channel() -> (
    SharedWorkerRuntimeOwnerWakeSender,
    mpsc::UnboundedReceiver<SharedWorkerRuntimeOwnerWake>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    (SharedWorkerRuntimeOwnerWakeSender::new(tx), rx)
}
