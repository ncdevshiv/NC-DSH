//! Cancellation and run supervision plumbing: [`RunHandle`] for the
//! supervisor, [`RunGuard`] for the worker.
//!
//! The pair is created together via [`RunHandle::pair`] and communicates in
//! both directions:
//!
//! - **supervisor → worker**: a [`CancellationToken`]; the supervisor's
//!   `cancel()` makes every `select!` site in the worker bail out.
//! - **worker → supervisor**: a `watch` channel carrying an
//!   `Option<TaskOutcome>`. The worker reports the real outcome through
//!   [`RunGuard::finish`]; if the guard is simply DROPPED without finishing,
//!   its [`Drop`] impl records [`TaskOutcome::Cancelled`] and cancels the
//!   token — an unfinished run is by definition a cancelled run.
//!
//! ```text
//!  supervisor                     worker
//!  ──────────                     ──────
//!  RunHandle ◀── pair ──▶ RunGuard
//!      │ cancel() ──▶ token ──▶ select! { _ = token.cancelled() => bail }
//!      ▼ try_result() ◀── watch ◀── finish(Success(..))
//! ```

use std::fmt;
use std::time::{Duration, Instant};

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::tree::TaskId;

/// How a supervised run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    /// The work completed; carries a self-contained summary.
    Success(String),
    /// The work failed; carries the error description.
    Failed(String),
    /// The run was cancelled (explicitly, or by losing its guard).
    Cancelled,
}

impl fmt::Display for TaskOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskOutcome::Success(s) => write!(f, "success: {s}"),
            TaskOutcome::Failed(e) => write!(f, "failed: {e}"),
            TaskOutcome::Cancelled => f.write_str("cancelled"),
        }
    }
}

/// Supervisor-side view of one delegated run: cancel it, watch it, read the
/// outcome. Deliberately not `Clone` — `try_result`/`result` track the watch
/// notification per reader; share by `Arc` if several tasks must observe.
#[derive(Debug)]
pub struct RunHandle {
    token: CancellationToken,
    outcome: watch::Receiver<Option<TaskOutcome>>,
    node: TaskId,
    started_at: Instant,
}

impl RunHandle {
    /// Creates a linked `(handle, guard)` pair for the given task node.
    #[must_use]
    pub fn pair(node: TaskId) -> (RunHandle, RunGuard) {
        let token = CancellationToken::new();
        let (outcome_tx, outcome_rx) = watch::channel(None);
        let handle = RunHandle {
            token: token.clone(),
            outcome: outcome_rx,
            node,
            started_at: Instant::now(),
        };
        let guard = RunGuard {
            token,
            outcome_tx,
            finished: false,
            node,
        };
        (handle, guard)
    }

    /// Requests cancellation of the running work. Idempotent and safe to
    /// call after the run already finished.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// A future resolving when cancellation is requested — drop it to stop
    /// watching without cancelling.
    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }

    /// The token itself, for handing to `tokio::select!` alongside other
    /// futures or sharing with additional worker tasks of the same run.
    #[must_use]
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// The task this run executes.
    #[must_use]
    pub fn node(&self) -> TaskId {
        self.node
    }

    /// When the run started.
    #[must_use]
    pub fn started_at(&self) -> Instant {
        self.started_at
    }

    /// Elapsed wall time since the run started.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Returns the outcome if the worker has reported one, consuming the
    /// "changed" notification (`None` while still running). Subsequent calls
    /// return the same value until a new outcome arrives.
    pub fn try_result(&mut self) -> Option<TaskOutcome> {
        self.outcome.borrow_and_update().clone()
    }

    /// Waits until the worker reports an outcome, returning it. Resolves
    /// immediately if one is already available. Infallible by construction:
    /// the guard's `Drop` always writes an outcome before the sender goes
    /// away, so the channel never closes silently.
    pub async fn result(&mut self) -> TaskOutcome {
        loop {
            if let Some(outcome) = self.outcome.borrow_and_update().clone() {
                return outcome;
            }
            self.outcome
                .changed()
                .await
                .expect("guard Drop always sends an outcome");
        }
    }
}

/// Worker-side counterpart of a [`RunHandle`]. Hold it for the duration of
/// the delegated work; report through [`finish`](RunGuard::finish), or just
/// drop it — dropping without finishing records `Cancelled` and cancels the
/// shared token so sibling workers of the same run stop too.
#[derive(Debug)]
pub struct RunGuard {
    token: CancellationToken,
    outcome_tx: watch::Sender<Option<TaskOutcome>>,
    finished: bool,
    node: TaskId,
}

impl RunGuard {
    /// Reports the final outcome of the run. Consumes the guard; dropping it
    /// afterwards is a no-op.
    pub fn finish(mut self, outcome: TaskOutcome) {
        self.finished = true;
        let _ = self.outcome_tx.send(Some(outcome));
        // NOTE: the token is NOT cancelled on success/failure — cancellation
        // remains supervisor-owned; observers select on whichever signal they
        // care about.
    }

    /// The task this guard runs.
    #[must_use]
    pub fn node(&self) -> TaskId {
        self.node
    }

    /// The shared token, for spawning helper tasks that should die with the
    /// run.
    #[must_use]
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Whether [`finish`](RunGuard::finish) has been called.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        if !self.finished {
            // An unfinished guard means the worker vanished mid-run: record
            // the cancellation and propagate it to every listener.
            let _ = self.outcome_tx.send(Some(TaskOutcome::Cancelled));
            self.token.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tokio::time::timeout;

    #[tokio::test]
    async fn spawned_task_honors_token_mid_work() {
        let (handle, guard) = RunHandle::pair(TaskId(1));

        let worker_token = guard.token().clone();
        let worker = tokio::spawn(async move {
            // Simulated long work, racing against cancellation.
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(60)) => "finished",
                _ = worker_token.cancelled() => "cancelled-out",
            }
        });

        assert!(!handle.is_cancelled());
        tokio::time::sleep(Duration::from_millis(20)).await;
        handle.cancel();

        let how_it_ended = worker.await.unwrap();
        assert_eq!(how_it_ended, "cancelled-out");

        // Supervisor observes the guard still held → no outcome yet.
        let mut handle = handle;
        assert_eq!(handle.try_result(), None);
        drop(guard); // worker vanished without finishing
        assert_eq!(handle.try_result(), Some(TaskOutcome::Cancelled));
    }

    #[tokio::test]
    async fn drop_without_finish_yields_cancelled_outcome() {
        let (mut handle, guard) = RunHandle::pair(TaskId(2));
        assert_eq!(handle.try_result(), None);
        drop(guard);
        // Outcome is immediately observable AND the waitable form resolves.
        assert_eq!(handle.try_result(), Some(TaskOutcome::Cancelled));
        assert_eq!(handle.result().await, TaskOutcome::Cancelled);
    }

    #[tokio::test]
    async fn finish_before_drop_preserves_real_outcome() {
        let (mut handle, guard) = RunHandle::pair(TaskId(3));
        guard.finish(TaskOutcome::Success("did the thing".into()));
        assert_eq!(
            handle.try_result(),
            Some(TaskOutcome::Success("did the thing".into()))
        );
        assert_eq!(
            handle.result().await,
            TaskOutcome::Success("did the thing".into())
        );
        assert_ne!(handle.try_result(), Some(TaskOutcome::Cancelled));
    }

    #[tokio::test]
    async fn failed_outcome_round_trips_through_watch() {
        let (mut handle, guard) = RunHandle::pair(TaskId(4));
        guard.finish(TaskOutcome::Failed("model exploded".into()));
        assert_eq!(
            handle.result().await,
            TaskOutcome::Failed("model exploded".into())
        );
    }

    #[tokio::test]
    async fn cancel_is_idempotent_and_safe_after_finish() {
        let (handle, guard) = RunHandle::pair(TaskId(5));
        handle.cancel();
        handle.cancel();
        handle.cancel();
        assert!(handle.is_cancelled());
        // Cancelling after the run finished must not corrupt anything.
        guard.finish(TaskOutcome::Success("done anyway".into()));
        handle.cancel();
        assert!(handle.is_cancelled());
        let mut handle = handle;
        assert_eq!(
            handle.try_result(),
            Some(TaskOutcome::Success("done anyway".into()))
        );
    }

    #[tokio::test]
    async fn cancelled_future_resolves_when_token_fires() {
        let (handle, _guard) = RunHandle::pair(TaskId(6));
        let shared = std::sync::Arc::new(handle);
        let watcher_handle = std::sync::Arc::clone(&shared);
        let watcher = tokio::spawn(async move { watcher_handle.cancelled().await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        shared.cancel();
        timeout(Duration::from_secs(1), watcher)
            .await
            .expect("watcher resolves after cancel")
            .unwrap();
        // Keep the pair alive until after the watcher observed the fire.
        assert!(shared.is_cancelled());
    }

    #[test]
    fn metadata_is_reported() {
        let (handle, guard) = RunHandle::pair(TaskId(9));
        assert_eq!(handle.node(), TaskId(9));
        assert_eq!(guard.node(), TaskId(9));
        assert!(!guard.is_finished());
        assert!(handle.started_at() <= Instant::now());
        assert!(handle.elapsed() < Duration::from_secs(60));
    }

    #[test]
    fn outcome_display_is_useful() {
        assert_eq!(TaskOutcome::Cancelled.to_string(), "cancelled");
        assert_eq!(TaskOutcome::Success("ok".into()).to_string(), "success: ok");
        assert_eq!(TaskOutcome::Failed("bad".into()).to_string(), "failed: bad");
    }
}
