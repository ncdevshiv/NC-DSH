use std::{num::NonZeroUsize, sync::Arc, thread, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use crate::{identity::DnsLookupKey, lookup::DnsLookup, state::DnsResolverState};

enum DnsWorkerCommand {
    Resolve(DnsLookupKey),
    Shutdown,
}

/// Fixed worker residence for blocking system resolver calls.
///
/// The queue owns admission pressure; one lookup cannot create a new OS thread.
/// Dropping a non-global test instance shuts down and joins every worker.
pub(crate) struct DnsWorkerPool {
    command_tx: Sender<DnsWorkerCommand>,
    worker_handles: Vec<thread::JoinHandle<()>>,
}

impl DnsWorkerPool {
    pub(crate) fn start(
        worker_count: NonZeroUsize,
        state: Arc<Mutex<DnsResolverState>>,
        lookup: Arc<DnsLookup>,
        positive_cache_ttl: Duration,
    ) -> std::io::Result<Self> {
        let (command_tx, command_rx) = crossbeam_channel::unbounded();
        let mut worker_handles: Vec<thread::JoinHandle<()>> =
            Vec::with_capacity(worker_count.get());
        for worker_index in 0..worker_count.get() {
            let state = Arc::clone(&state);
            let command_rx = command_rx.clone();
            let lookup = Arc::clone(&lookup);
            let thread_name = format!("lm-dns-{}", worker_index + 1);
            let worker_handle = match thread::Builder::new()
                .name(thread_name)
                .spawn(move || run_worker(command_rx, state, lookup, positive_cache_ttl))
            {
                Ok(worker_handle) => worker_handle,
                Err(error) => {
                    stop_workers(&command_tx, &mut worker_handles);
                    return Err(error);
                }
            };
            worker_handles.push(worker_handle);
        }
        Ok(Self {
            command_tx,
            worker_handles,
        })
    }

    pub(crate) fn resolve(
        &self,
        key: DnsLookupKey,
    ) -> Result<(), crossbeam_channel::SendError<DnsLookupKey>> {
        self.command_tx
            .send(DnsWorkerCommand::Resolve(key))
            .map_err(|error| match error.into_inner() {
                DnsWorkerCommand::Resolve(key) => crossbeam_channel::SendError(key),
                DnsWorkerCommand::Shutdown => {
                    unreachable!("resolve never submits a shutdown command")
                }
            })
    }
}

impl Drop for DnsWorkerPool {
    fn drop(&mut self) {
        stop_workers(&self.command_tx, &mut self.worker_handles);
    }
}

fn stop_workers(
    command_tx: &Sender<DnsWorkerCommand>,
    worker_handles: &mut Vec<thread::JoinHandle<()>>,
) {
    for _ in 0..worker_handles.len() {
        let _ = command_tx.send(DnsWorkerCommand::Shutdown);
    }
    for worker_handle in worker_handles.drain(..) {
        let _ = worker_handle.join();
    }
}

fn run_worker(
    command_rx: Receiver<DnsWorkerCommand>,
    state: Arc<Mutex<DnsResolverState>>,
    lookup: Arc<DnsLookup>,
    positive_cache_ttl: Duration,
) {
    while let Ok(command) = command_rx.recv() {
        match command {
            DnsWorkerCommand::Resolve(key) => {
                let result = lookup(&key.target);
                publish_lookup_result(&state, key, result, positive_cache_ttl);
            }
            DnsWorkerCommand::Shutdown => return,
        }
    }
}

pub(crate) fn publish_lookup_result(
    state: &Mutex<DnsResolverState>,
    key: DnsLookupKey,
    result: crate::DnsLookupResult,
    positive_cache_ttl: Duration,
) {
    let completions =
        state
            .lock()
            .finish(key, &result, std::time::Instant::now() + positive_cache_ttl);
    for completion in completions {
        completion(result.clone());
    }
}
