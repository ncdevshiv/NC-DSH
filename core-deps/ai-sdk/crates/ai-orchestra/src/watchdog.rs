//! The supervisor's senses: progress bookkeeping and stall classification.
//!
//! The watchdog answers one question per active run: *is this run healthy?*
//! It is deliberately split in two:
//!
//! - A **progress feed** ([`ProgressLedger`]): workers (or tests) report
//!   observable progress by hashing a progress string with
//!   [`std::collections::hash_map::DefaultHasher`]. Identical consecutive
//!   hashes are the raw material of loop detection; the timestamp of the
//!   last report drives stall detection.
//! - A **pure classifier** ([`Watchdog::evaluate`]): given distilled
//!   measurements of one run, decide whether it is stalled and what to do
//!   about it. The function is total and side-effect free, so every rule is
//!   unit-testable without sleeping.
//!
//! [`Watchdog::sweep`] glues the two together over a live handle map. One
//! action per task per sweep, evaluated in priority order:
//!
//! ```text
//! 1. elapsed > hard_deadline  OR cancelled-stuck  ⇒ OverranDeadline → Reassign
//! 2. now − last_progress > progress_window        ⇒ NoProgress      → Retry
//! 3. last K signature hashes equal AND K ≥ min    ⇒ LoopSignature   → RespawnAmended
//! 4. budget hook (placeholder, disabled)          ⇒ BudgetBurn      → Nudge
//! ```
//!
//! # Budget-burn placeholder
//!
//! `BudgetBurn` is intentionally a **configuration placeholder**
//! (`WatchdogConfig::budget_burn_enabled`, default `false`). Token/cost
//! accounting does not exist in this crate yet, so the hook is documented,
//! typed, classified (it fires `Nudge` when explicitly enabled), and
//! unit-tested — but no orchestrator wiring enables it. When a usage meter
//! lands, flip the flag and feed real burn numbers through
//! [`DiagnosisInput`] without touching callers.

use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::handle::RunHandle;
use crate::tree::TaskId;

/// Signatures kept per task; old entries fall off a rolling window so the
/// tail always reflects the most recent [`MAX_TRACKED_SIGNATURES`] reports.
pub const MAX_TRACKED_SIGNATURES: usize = 8;

/// What kind of unhealthy behaviour was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallKind {
    /// No observable progress within the configured window.
    NoProgress,
    /// The run exceeded its hard deadline, or ignored cancellation for too
    /// long ("cancelled-stuck").
    OverranDeadline,
    /// Placeholder for future budget/burn accounting (see module docs).
    BudgetBurn,
    /// The last K progress signatures are identical — likely an output loop.
    LoopSignature,
}

impl std::fmt::Display for StallKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            StallKind::NoProgress => "no-progress",
            StallKind::OverranDeadline => "overran-deadline",
            StallKind::BudgetBurn => "budget-burn",
            StallKind::LoopSignature => "loop-signature",
        };
        f.write_str(name)
    }
}

/// How urgently a [`SupervisionAction`] should be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational; no correctness impact.
    Info,
    /// Suspicious; recovery is cheap and safe.
    Warning,
    /// Almost certainly broken; disruptive recovery is justified.
    Critical,
}

/// What the supervisor suggests doing about a stalled run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryPolicy {
    /// Log-only intervention: nudge the run, change nothing.
    Nudge,
    /// Fail and retry the task from scratch.
    Retry,
    /// Amend the node's brief with a hint, then retry.
    RespawnAmended(String),
    /// Kill the current run and re-delegate to a different worker.
    Reassign,
    /// Give up automatically: park for a human, carrying the reason.
    Escalate(String),
}

/// One watchdog verdict for one active run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisionAction {
    /// The run's task.
    pub task: TaskId,
    /// Which rule fired.
    pub kind: StallKind,
    /// Urgency of the verdict.
    pub severity: Severity,
    /// Suggested recovery.
    pub recommended: RecoveryPolicy,
}

impl SupervisionAction {
    /// Convenience constructor used by the classifier and tests alike.
    #[must_use]
    pub fn new(
        task: TaskId,
        kind: StallKind,
        severity: Severity,
        recommended: RecoveryPolicy,
    ) -> Self {
        Self {
            task,
            kind,
            severity,
            recommended,
        }
    }
}

/// Per-run progress state maintained by the feed.
#[derive(Debug, Clone)]
pub struct ProgressRecord {
    /// When progress was last reported.
    pub last_at: Instant,
    /// Hashes of the most recent progress strings (rolling window).
    pub signatures: Vec<u64>,
}

/// Shared progress feed: workers report here, the watchdog reads here.
///
/// Cloning shares the same underlying map (`Arc<Mutex<..>>`), which is how
/// the orchestrator hands the SAME ledger to its workers and to the
/// watchdog. Tests may also seed it directly to exercise loop detection —
/// honest because `Agent::run` exposes no per-iteration callback, so
/// production feeds only start/end markers while tests supply richer
/// sequences through this exact public surface.
#[derive(Debug, Clone, Default)]
pub struct ProgressLedger(Arc<Mutex<HashMap<TaskId, ProgressRecord>>>);

/// Hashes a progress string with std's [`DefaultHasher`].
#[must_use]
pub fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

impl ProgressLedger {
    /// Creates an empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a progress observation for `task`: refreshes `last_at` and
    /// appends `hash_string(hashable)` to the rolling signature window.
    pub fn note_progress(&self, task: TaskId, hashable: &str) {
        let mut ledger = self.0.lock();
        let record = ledger.entry(task).or_insert_with(|| ProgressRecord {
            last_at: Instant::now(),
            signatures: Vec::new(),
        });
        record.last_at = Instant::now();
        record.signatures.push(hash_string(hashable));
        let excess = record
            .signatures
            .len()
            .saturating_sub(MAX_TRACKED_SIGNATURES);
        if excess > 0 {
            record.signatures.drain(..excess);
        }
    }

    /// Snapshot of one task's record, if any progress was ever reported.
    #[must_use]
    pub fn record(&self, task: TaskId) -> Option<ProgressRecord> {
        self.0.lock().get(&task).cloned()
    }

    /// The last `k` recorded signature hashes (fewer if not yet available).
    #[must_use]
    pub fn signature_tail(&self, task: TaskId, k: usize) -> Vec<u64> {
        self.record(task)
            .map(|r| {
                let start = r.signatures.len().saturating_sub(k);
                r.signatures[start..].to_vec()
            })
            .unwrap_or_default()
    }

    /// Time since the last progress report (`None` = never reported).
    #[must_use]
    pub fn since_last_progress(&self, task: TaskId, now: Instant) -> Option<Duration> {
        self.record(task)
            .map(|r| now.saturating_duration_since(r.last_at))
    }

    /// Test/ops convenience: force-sign specific hashes as if reported just
    /// now. Exercises loop detection without waiting for real repeats.
    pub fn seed_signatures(&self, task: TaskId, hashes: &[u64]) {
        let mut ledger = self.0.lock();
        let record = ledger.entry(task).or_insert_with(|| ProgressRecord {
            last_at: Instant::now(),
            signatures: Vec::new(),
        });
        record.last_at = Instant::now();
        record.signatures.extend_from_slice(hashes);
        let excess = record
            .signatures
            .len()
            .saturating_sub(MAX_TRACKED_SIGNATURES);
        if excess > 0 {
            record.signatures.drain(..excess);
        }
    }
}

/// Watchdog tuning knobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchdogConfig {
    /// Fire [`StallKind::NoProgress`] when a run reports nothing for this
    /// long. Default 30s.
    pub progress_window: Duration,
    /// Fire [`StallKind::OverranDeadline`] past this wall time. Default 5min.
    pub hard_deadline: Duration,
    /// Consecutive identical signatures required for
    /// [`StallKind::LoopSignature`]. Default 3.
    pub loop_signature_min_repeats: usize,
    /// How often the supervisor sweeps active runs. Default 100ms.
    pub sweep_interval: Duration,
    /// Budget-burn placeholder flag — inert by default, see module docs.
    /// Default false.
    pub budget_burn_enabled: bool,
    /// Budget-burn placeholder threshold used only when
    /// `budget_burn_enabled` is true. Default 60s.
    pub budget_window: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            progress_window: Duration::from_secs(30),
            hard_deadline: Duration::from_secs(300),
            loop_signature_min_repeats: 3,
            sweep_interval: Duration::from_millis(100),
            budget_burn_enabled: false,
            budget_window: Duration::from_secs(60),
        }
    }
}

/// Distilled measurements of ONE active run — everything the pure
/// classifier needs, nothing else. Building this from live handles is
/// [`Watchdog::sweep`]'s job; tests build it directly.
#[derive(Debug, Clone, Copy)]
pub struct DiagnosisInput<'a> {
    /// Wall time since the run started.
    pub elapsed: Duration,
    /// True when the run was asked to cancel but has ignored it long enough
    /// to be considered stuck (the sweep derives this from
    /// `is_cancelled() && elapsed > progress_window`; the orchestrator
    /// retires handles promptly on outcome, so a lingering cancelled handle
    /// IS stuck by construction).
    pub cancelled_stuck: bool,
    /// Time since the last progress report; `None` when the run never
    /// reported (rule skipped — cannot judge what has no baseline).
    pub since_progress: Option<Duration>,
    /// Most recent signature hashes (already windowed by the ledger).
    pub signature_tail: &'a [u64],
}

/// Stateless rule engine over a shared [`ProgressLedger`].
#[derive(Debug, Clone)]
pub struct Watchdog {
    config: WatchdogConfig,
    ledger: ProgressLedger,
}

impl Watchdog {
    /// Creates a watchdog reading `ledger` under `config`. The ledger is
    /// shared by reference-count with whoever feeds progress.
    #[must_use]
    pub fn new(config: WatchdogConfig, ledger: ProgressLedger) -> Self {
        Self { config, ledger }
    }

    /// The active configuration.
    #[must_use]
    pub fn config(&self) -> &WatchdogConfig {
        &self.config
    }

    /// The shared progress feed (orchestrator workers report through this).
    #[must_use]
    pub fn ledger(&self) -> &ProgressLedger {
        &self.ledger
    }

    /// Pure classification of one run. Priority order is fixed:
    /// deadline → no-progress → loop-signature → budget placeholder. At
    /// most one action per call.
    #[must_use]
    pub fn evaluate(&self, task: TaskId, input: DiagnosisInput<'_>) -> Option<SupervisionAction> {
        // 1. Hard deadline / cancelled-stuck: the run must die and go elsewhere.
        if input.elapsed > self.config.hard_deadline || input.cancelled_stuck {
            return Some(SupervisionAction::new(
                task,
                StallKind::OverranDeadline,
                Severity::Critical,
                RecoveryPolicy::Reassign,
            ));
        }
        // 2. Silent run: nothing observed inside the progress window.
        if let Some(since) = input.since_progress {
            if since > self.config.progress_window {
                return Some(SupervisionAction::new(
                    task,
                    StallKind::NoProgress,
                    Severity::Warning,
                    RecoveryPolicy::Retry,
                ));
            }
        }
        // 3. Repeating output: last K signatures all identical with K ≥ min.
        let k = self.config.loop_signature_min_repeats;
        if k > 0 && input.signature_tail.len() >= k {
            let tail = &input.signature_tail[input.signature_tail.len() - k..];
            if tail.iter().all(|h| *h == tail[0]) {
                return Some(SupervisionAction::new(
                    task,
                    StallKind::LoopSignature,
                    Severity::Critical,
                    RecoveryPolicy::RespawnAmended("repeating identical output".into()),
                ));
            }
        }
        // 4. Budget placeholder: only fires when explicitly enabled; the
        //    gentlest policy (Nudge) since burn alone is not proof of failure.
        if self.config.budget_burn_enabled && input.elapsed > self.config.budget_window {
            return Some(SupervisionAction::new(
                task,
                StallKind::BudgetBurn,
                Severity::Info,
                RecoveryPolicy::Nudge,
            ));
        }
        None
    }

    /// Classifies every active run. Pure reads plus clock reads — no
    /// mutation, no awaiting, safe to call under the orchestrator's state
    /// lock.
    ///
    /// Runs whose guard vanished are absent from `active` by construction
    /// (the orchestrator retires them on completion), so "cancelled-stuck"
    /// reduces to: token fired, handle still here, well past the progress
    /// window.
    #[must_use]
    pub fn sweep(&self, active: &HashMap<TaskId, RunHandle>) -> Vec<SupervisionAction> {
        let now = Instant::now();
        let mut actions = Vec::new();
        for (task, handle) in active {
            let cancelled_stuck =
                handle.is_cancelled() && handle.elapsed() > self.config.progress_window;
            // Owned local so the immutable borrow outlives the struct literal.
            let tail = self.ledger.signature_tail(*task, MAX_TRACKED_SIGNATURES);
            let input = DiagnosisInput {
                elapsed: handle.elapsed(),
                cancelled_stuck,
                since_progress: self.ledger.since_last_progress(*task, now),
                signature_tail: &tail,
            };
            if let Some(action) = self.evaluate(*task, input) {
                actions.push(action);
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    fn cfg() -> WatchdogConfig {
        WatchdogConfig {
            progress_window: Duration::from_millis(20),
            hard_deadline: Duration::from_millis(50),
            loop_signature_min_repeats: 3,
            sweep_interval: Duration::from_millis(5),
            ..WatchdogConfig::default()
        }
    }

    fn dog() -> Watchdog {
        Watchdog::new(cfg(), ProgressLedger::new())
    }

    fn input<'a>(
        elapsed_ms: u64,
        cancelled_stuck: bool,
        since_progress: Option<u64>,
        tail: &'a [u64],
    ) -> DiagnosisInput<'a> {
        DiagnosisInput {
            elapsed: Duration::from_millis(elapsed_ms),
            cancelled_stuck,
            since_progress: since_progress.map(Duration::from_millis),
            signature_tail: tail,
        }
    }

    #[test]
    fn defaults_match_spec() {
        let c = WatchdogConfig::default();
        assert_eq!(c.progress_window, Duration::from_secs(30));
        assert_eq!(c.hard_deadline, Duration::from_secs(300));
        assert_eq!(c.loop_signature_min_repeats, 3);
        assert_eq!(c.sweep_interval, Duration::from_millis(100));
        assert!(!c.budget_burn_enabled);
    }

    #[test]
    fn healthy_run_produces_no_action() {
        assert_eq!(
            dog().evaluate(TaskId(1), input(10, false, Some(5), &[1, 2, 3])),
            None
        );
        // No progress baseline at all yet (never reported): rule skipped.
        assert_eq!(dog().evaluate(TaskId(1), input(10, false, None, &[])), None);
    }

    #[test]
    fn overran_deadline_recommends_reassign() {
        let action = dog()
            .evaluate(TaskId(2), input(60, false, Some(1), &[]))
            .expect("deadline exceeded");
        assert_eq!(action.kind, StallKind::OverranDeadline);
        assert_eq!(action.severity, Severity::Critical);
        assert_eq!(action.recommended, RecoveryPolicy::Reassign);
    }

    #[test]
    fn cancelled_stuck_counts_as_deadline_breach() {
        let action = dog()
            .evaluate(TaskId(3), input(30, true, Some(1), &[]))
            .expect("cancelled-stuck");
        assert_eq!(action.kind, StallKind::OverranDeadline);
        assert_eq!(action.recommended, RecoveryPolicy::Reassign);
        // Cancelled but only briefly: the SWEEP would distill that to
        // `cancelled_stuck == false` (stuck requires outliving the window),
        // so at classifier level nothing fires.
        assert_eq!(
            dog().evaluate(TaskId(3), input(5, false, Some(1), &[])),
            None
        );
    }

    #[test]
    fn silent_run_recommends_retry() {
        let action = dog()
            .evaluate(TaskId(4), input(10, false, Some(25), &[9]))
            .expect("progress window blown");
        assert_eq!(action.kind, StallKind::NoProgress);
        assert_eq!(action.severity, Severity::Warning);
        assert_eq!(action.recommended, RecoveryPolicy::Retry);
    }

    #[test]
    fn identical_signature_tail_fires_loop_signature() {
        let dog = dog();
        let same = [7_u64, 7, 7];
        let action = dog
            .evaluate(TaskId(5), input(10, false, Some(0), &same))
            .expect("loop signature");
        assert_eq!(action.kind, StallKind::LoopSignature);
        assert_eq!(action.severity, Severity::Critical);
        assert_eq!(
            action.recommended,
            RecoveryPolicy::RespawnAmended("repeating identical output".into())
        );
        // Fewer than min repeats: silent even when identical.
        assert_eq!(
            dog.evaluate(TaskId(5), input(10, false, Some(0), &[7, 7])),
            None
        );
        // Enough repeats but not all equal: silent.
        let mixed = [7_u64, 8, 7];
        assert_eq!(
            dog.evaluate(TaskId(5), input(10, false, Some(0), &mixed)),
            None
        );
        // Longer tail whose LAST k are identical still fires.
        let long = [1_u64, 2, 7, 7, 7];
        assert!(
            dog.evaluate(TaskId(5), input(10, false, Some(0), &long))
                .is_some()
        );
    }

    #[test]
    fn priority_is_deadline_then_progress_then_loop() {
        let dog = dog();
        let all_bad = [7_u64, 7, 7];
        // Everything wrong at once → deadline wins.
        assert_eq!(
            dog.evaluate(TaskId(6), input(100, true, Some(90), &all_bad))
                .unwrap()
                .kind,
            StallKind::OverranDeadline
        );
        // Deadline fine, progress + loop broken → no-progress wins.
        assert_eq!(
            dog.evaluate(TaskId(6), input(10, false, Some(90), &all_bad))
                .unwrap()
                .kind,
            StallKind::NoProgress
        );
    }

    #[test]
    fn budget_burn_placeholder_only_when_enabled_and_gentle() {
        let mut config = cfg();
        config.budget_burn_enabled = true;
        config.budget_window = Duration::from_millis(40);
        let budget_dog = Watchdog::new(config, ProgressLedger::new());
        // Under budget: nothing.
        assert_eq!(
            budget_dog.evaluate(TaskId(7), input(10, false, Some(1), &[])),
            None
        );
        let action = budget_dog
            .evaluate(TaskId(7), input(45, false, Some(1), &[]))
            .expect("budget placeholder");
        assert_eq!(action.kind, StallKind::BudgetBurn);
        assert_eq!(action.severity, Severity::Info);
        assert_eq!(action.recommended, RecoveryPolicy::Nudge);
        // Disabled by default: never fires.
        assert_eq!(
            dog()
                .evaluate(TaskId(7), input(999_999, false, Some(1), &[]))
                .map(|a| a.kind),
            Some(StallKind::OverranDeadline)
        );
    }

    #[test]
    fn hash_string_is_deterministic_and_discriminating() {
        assert_eq!(hash_string("alpha"), hash_string("alpha"));
        assert_ne!(hash_string("alpha"), hash_string("beta"));
    }

    #[test]
    fn ledger_tracks_time_signatures_and_windows() {
        let ledger = ProgressLedger::new();
        assert!(ledger.record(TaskId(1)).is_none());

        ledger.note_progress(TaskId(1), "step-a");
        ledger.note_progress(TaskId(1), "step-b");
        assert_eq!(ledger.signature_tail(TaskId(1), 2).len(), 2);
        assert_ne!(
            ledger.signature_tail(TaskId(1), 2)[0],
            ledger.signature_tail(TaskId(1), 2)[1],
            "distinct strings must hash differently"
        );
        assert!(
            ledger
                .since_last_progress(TaskId(1), Instant::now())
                .is_some()
        );

        // Rolling window caps the retained signatures.
        for i in 0..(MAX_TRACKED_SIGNATURES + 4) {
            ledger.note_progress(TaskId(1), &format!("gen-{i}"));
        }
        let record = ledger.record(TaskId(1)).unwrap();
        assert_eq!(record.signatures.len(), MAX_TRACKED_SIGNATURES);

        // Tasks are independent.
        assert!(ledger.record(TaskId(2)).is_none());
    }

    #[tokio::test]
    async fn sweep_classifies_live_handles_with_short_durations() {
        let ledger = ProgressLedger::new();
        let config = WatchdogConfig {
            progress_window: Duration::from_millis(15),
            hard_deadline: Duration::from_secs(600),
            sweep_interval: Duration::from_millis(1),
            ..WatchdogConfig::default()
        };
        let dog = Watchdog::new(config.clone(), ledger.clone());

        let (healthy, healthy_guard) = RunHandle::pair(TaskId(10));
        let (silent, _silent_guard) = RunHandle::pair(TaskId(11)); // guard held: alive
        ledger.note_progress(TaskId(10), "started");
        ledger.note_progress(TaskId(11), "started");

        let (cancelled, cancelled_guard) = RunHandle::pair(TaskId(12));
        ledger.note_progress(TaskId(12), "started");

        sleep(Duration::from_millis(30));
        cancelled.cancel(); // ignored by the (held) guard → stuck after window
        // The healthy run keeps reporting; the silent one does not.
        ledger.note_progress(TaskId(10), "still working");

        let mut active = HashMap::new();
        active.insert(TaskId(10), healthy);
        active.insert(TaskId(11), silent);
        active.insert(TaskId(12), cancelled);

        let actions = dog.sweep(&active);
        let kinds: Vec<(TaskId, StallKind)> = actions.iter().map(|a| (a.task, a.kind)).collect();
        // Healthy run: reported recently, nothing fires.
        assert!(!kinds.iter().any(|(t, _)| *t == TaskId(10)));
        // Silent past the window → Retry.
        assert_eq!(
            kinds
                .iter()
                .find(|(t, _)| *t == TaskId(11))
                .map(|(_, k)| *k),
            Some(StallKind::NoProgress)
        );
        // Cancelled and stuck → Reassign.
        let stuck = actions.iter().find(|a| a.task == TaskId(12)).unwrap();
        assert_eq!(stuck.kind, StallKind::OverranDeadline);
        assert_eq!(stuck.recommended, RecoveryPolicy::Reassign);

        drop(cancelled_guard);
        drop(healthy_guard);
    }

    #[test]
    fn sweep_of_empty_map_is_empty() {
        let dog = Watchdog::new(WatchdogConfig::default(), ProgressLedger::new());
        assert!(dog.sweep(&HashMap::new()).is_empty());
    }
}
