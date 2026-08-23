use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::warn;

use crate::worker::WorkerToParentMessage;

use super::{host::RendererSharedWorkerHost, threads::shared_worker_thread_name};

pub(super) fn drain_shared_worker_parent_messages(
    host: Arc<RendererSharedWorkerHost>,
    script_url: String,
    rx: Option<mpsc::UnboundedReceiver<WorkerToParentMessage>>,
) {
    let Some(mut rx) = rx else {
        return;
    };
    let cleanup_host = Arc::clone(&host);
    let pump_script_url = script_url.clone();
    if let Err(error) = std::thread::Builder::new()
        .name(shared_worker_thread_name("sw-pump", host.instance_id()))
        .spawn(move || {
            while let Some(message) = rx.blocking_recv() {
                host.handle_worker_parent_message(message, &pump_script_url);
            }
            host.notify_worker_closed();
        })
    {
        warn!(
            url = %script_url,
            instance_id = cleanup_host.instance_id().as_u64(),
            %error,
            "failed to spawn shared worker parent-message pump; closing shared worker host"
        );
        cleanup_host.notify_worker_closed();
    }
}
