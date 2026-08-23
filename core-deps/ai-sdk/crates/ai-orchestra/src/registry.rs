//! The agent pool: register workers once, acquire them by specialty, release
//! them for reuse.
//!
//! The pool is deliberately adapter-shaped: [`WorkerAdapter`] abstracts "a
//! thing that can run tasks", so wave B can pool plain agents, derived
//! per-task agents, or entirely different executors behind one registry.
//! [`AgentEntry`] is the canonical implementation over
//! `Arc<ai_agents::Agent>`; [`derive_entry`] mints isolated per-task agents
//! from a configured base via `Agent::derive` (fresh memory, inherited
//! model/tools).
//!
//! Acquisition is atomic: [`WorkerAdapter::try_claim`] flips idle → busy with
//! a single `compare_exchange`, so concurrent supervisors can never
//! double-book an entry even though acquisition only takes the registry's
//! read lock. Pool exhaustion returns `None` — growing the pool on demand is
//! wave-B policy (factory composition), not this module's concern.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ai_agents::Agent;
use parking_lot::RwLock;

/// A pooled executor abstraction. Implementations must be usable from
/// multiple threads; busy-state transitions are interior and lock-free.
pub trait WorkerAdapter: Send + Sync {
    /// Stable id of this worker (unique within its registry).
    fn agent_id(&self) -> &str;

    /// Specialty tags used for preference-aware acquisition.
    fn specialties(&self) -> &[String];

    /// Whether the worker is currently free.
    fn is_idle(&self) -> bool;

    /// Marks the worker busy (`true`) or idle (`false`).
    fn set_busy(&self, busy: bool);

    /// The underlying agent, when this adapter wraps one.
    fn as_agent(&self) -> Option<&Arc<Agent>>;

    /// Atomically claims the worker if idle, marking it busy. `true` means
    /// THIS call won the claim.
    ///
    /// The default is a non-atomic check-then-set, correct under the
    /// registry's write lock but racy otherwise; implementations with an
    /// atomic busy flag (like [`AgentEntry`]) override it with a CAS so
    /// acquisition stays safe under the read lock.
    fn try_claim(&self) -> bool {
        if self.is_idle() {
            self.set_busy(true);
            true
        } else {
            false
        }
    }
}

/// Canonical [`WorkerAdapter`] over an optional `Arc<Agent>` plus specialty
/// tags. The agent is `None` for placeholder entries (e.g. capacity markers
/// created before their agent is attached).
#[derive(Debug)]
pub struct AgentEntry {
    agent: Option<Arc<Agent>>,
    id: String,
    specialties: Vec<String>,
    busy: AtomicBool,
}

impl AgentEntry {
    /// Creates an entry around an existing agent.
    #[must_use]
    pub fn new(agent: Arc<Agent>, specialties: Vec<String>) -> Self {
        let id = agent.id().to_owned();
        Self {
            agent: Some(agent),
            id,
            specialties,
            busy: AtomicBool::new(false),
        }
    }

    /// Creates an id-only placeholder with no backing agent.
    #[must_use]
    pub fn placeholder(id: impl Into<String>, specialties: Vec<String>) -> Self {
        Self {
            agent: None,
            id: id.into(),
            specialties,
            busy: AtomicBool::new(false),
        }
    }

    fn try_claim_inner(&self) -> bool {
        self.busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

impl WorkerAdapter for AgentEntry {
    fn agent_id(&self) -> &str {
        &self.id
    }

    fn specialties(&self) -> &[String] {
        &self.specialties
    }

    fn is_idle(&self) -> bool {
        !self.busy.load(Ordering::SeqCst)
    }

    fn set_busy(&self, busy: bool) {
        self.busy.store(busy, Ordering::SeqCst);
    }

    fn as_agent(&self) -> Option<&Arc<Agent>> {
        self.agent.as_ref()
    }

    fn try_claim(&self) -> bool {
        self.try_claim_inner()
    }
}

/// Mints a fresh pooled entry from a configured base agent.
///
/// Calls [`Agent::derive`], producing an agent whose id is `{base_id}{suffix}`
/// with FRESH memory (swarm-style isolation); the derived agent shares the
/// base's model/tools/collector cheaply via `Arc`. `specialties` tag the new
/// entry for acquisition preference.
#[must_use]
pub fn derive_entry(base: &Arc<Agent>, id_suffix: &str, specialties: Vec<String>) -> AgentEntry {
    AgentEntry::new(Arc::new(base.derive(id_suffix)), specialties)
}

/// Snapshot of pool occupancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RegistryStats {
    /// Total registered entries.
    pub total: usize,
    /// Entries currently free.
    pub idle: usize,
    /// Entries currently checked out.
    pub busy: usize,
}

/// Errors returned by registry operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// An entry with this id is already registered.
    #[error("agent '{0}' is already registered")]
    DuplicateId(String),
    /// No entry with this id exists.
    #[error("unknown agent '{0}'")]
    UnknownAgent(String),
}

/// A concurrency-safe pool of [`WorkerAdapter`]s with reuse and
/// specialty-preference acquisition.
///
/// Locking: one `RwLock<Vec<Arc<entry>>>`. Registration/removal takes the
/// write lock; acquisition/release take only a read lock (the busy flag does
/// the mutual exclusion atomically via `try_claim`).
#[derive(Default)]
pub struct AgentRegistry {
    entries: RwLock<Vec<Arc<dyn WorkerAdapter>>>,
}

impl std::fmt::Debug for AgentRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentRegistry")
            .field("ids", &self.ids())
            .finish()
    }
}

impl AgentRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an entry. Rejects duplicate ids.
    pub fn register(&self, entry: impl IntoPoolEntry) -> Result<(), RegistryError> {
        let entry = entry.into_pool_entry();
        let mut entries = self.entries.write();
        if entries.iter().any(|e| e.agent_id() == entry.agent_id()) {
            return Err(RegistryError::DuplicateId(entry.agent_id().to_owned()));
        }
        entries.push(entry);
        Ok(())
    }

    /// Acquires an idle entry, preferring ones whose specialties intersect
    /// `preferred` (ANY match wins); falls back to any idle entry when no
    /// specialist is free. Returns `None` when the pool is exhausted.
    ///
    /// On success the entry is atomically marked busy — the same entry can
    /// never be handed to two acquirers.
    pub fn acquire(&self, preferred: &[&str]) -> Option<Arc<dyn WorkerAdapter>> {
        let entries = self.entries.read();

        // Pass 1: idle AND specialty-matching (earliest registered wins).
        for entry in entries.iter() {
            let matches = entry
                .specialties()
                .iter()
                .any(|s| preferred.contains(&s.as_str()));
            if matches && entry.try_claim() {
                return Some(Arc::clone(entry));
            }
        }
        // Pass 2: any idle entry.
        for entry in entries.iter() {
            if entry.try_claim() {
                return Some(Arc::clone(entry));
            }
        }
        None
    }

    /// Releases a previously acquired entry back to the pool (marks idle).
    /// Returns whether the entry was actually busy before the call.
    pub fn release(&self, entry: &Arc<dyn WorkerAdapter>) -> bool {
        if entry.is_idle() {
            return false;
        }
        entry.set_busy(false);
        true
    }

    /// Removes an entry by id, returning it.
    pub fn remove(&self, id: &str) -> Result<Arc<dyn WorkerAdapter>, RegistryError> {
        let mut entries = self.entries.write();
        let index = entries
            .iter()
            .position(|e| e.agent_id() == id)
            .ok_or_else(|| RegistryError::UnknownAgent(id.to_owned()))?;
        Ok(entries.remove(index))
    }

    /// Look up an entry by id without acquiring it.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Arc<dyn WorkerAdapter>> {
        self.entries
            .read()
            .iter()
            .find(|e| e.agent_id() == id)
            .map(Arc::clone)
    }

    /// Occupancy snapshot.
    pub fn stats(&self) -> RegistryStats {
        let entries = self.entries.read();
        RegistryStats {
            total: entries.len(),
            idle: entries.iter().filter(|e| e.is_idle()).count(),
            busy: entries.iter().filter(|e| !e.is_idle()).count(),
        }
    }

    /// All ids, registration order preserved.
    pub fn ids(&self) -> Vec<String> {
        self.entries
            .read()
            .iter()
            .map(|e| e.agent_id().to_owned())
            .collect()
    }
}

/// Convenience trait so [`AgentRegistry::register`] accepts both concrete
/// entries and pre-wrapped trait objects.
pub trait IntoPoolEntry {
    /// Converts into the pooled representation.
    fn into_pool_entry(self) -> Arc<dyn WorkerAdapter>;
}

impl IntoPoolEntry for AgentEntry {
    fn into_pool_entry(self) -> Arc<dyn WorkerAdapter> {
        Arc::new(self)
    }
}

impl IntoPoolEntry for Arc<dyn WorkerAdapter> {
    fn into_pool_entry(self) -> Arc<dyn WorkerAdapter> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ai_agents::AgentBuilder;
    use ai_core::{ChatRequest, Completion, EventStream, Model};
    use ai_errors::AiError;
    use ai_models::{ModelCapabilities, ModelInfo};
    use ai_types::{ModelId, ProviderId};

    // -- offline scripted model (per ADR-007: unit tests mock the LLM) -----

    fn model_info() -> &'static ModelInfo {
        static INFO: std::sync::OnceLock<ModelInfo> = std::sync::OnceLock::new();
        INFO.get_or_init(|| {
            ModelInfo::new(
                ProviderId::new("test"),
                ModelId::new("scripted"),
                128_000,
                8_192,
            )
            .with_capabilities(ModelCapabilities::default())
        })
    }

    /// Never actually run: registry tests exercise pooling, not generation.
    struct NullModel;

    #[async_trait::async_trait]
    impl Model for NullModel {
        fn info(&self) -> &ModelInfo {
            model_info()
        }

        async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
            unreachable!("registry tests never generate")
        }

        async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
            unreachable!("registry tests never stream")
        }
    }

    fn scripted_agent(id: &str) -> Arc<Agent> {
        Arc::new(AgentBuilder::new(id, "test instructions", Arc::new(NullModel)).build())
    }

    fn pool_with(ids: &[(&str, &[&str])]) -> AgentRegistry {
        let reg = AgentRegistry::new();
        for (id, specs) in ids {
            let specialties = specs.iter().map(|s| (*s).to_owned()).collect();
            reg.register(AgentEntry::new(scripted_agent(id), specialties))
                .unwrap();
        }
        reg
    }

    // -- basic pooling ------------------------------------------------------

    #[test]
    fn register_and_lookup() {
        let reg = pool_with(&[("a", &["rust"]), ("b", &["python"])]);

        assert_eq!(reg.ids(), vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(
            reg.stats(),
            RegistryStats {
                total: 2,
                idle: 2,
                busy: 0
            }
        );

        let found = reg.get("b").unwrap();
        assert_eq!(found.agent_id(), "b");
        assert!(found.as_agent().is_some());

        // Duplicate registration is rejected with a typed error.
        assert_eq!(
            reg.register(AgentEntry::placeholder("a", vec![]))
                .unwrap_err(),
            RegistryError::DuplicateId("a".into())
        );
        assert_eq!(
            reg.remove("ghost").err().unwrap(),
            RegistryError::UnknownAgent("ghost".into())
        );

        let removed = reg.remove("a").unwrap();
        assert_eq!(removed.agent_id(), "a");
        assert_eq!(reg.stats().total, 1);
    }

    #[test]
    fn derive_entry_mints_fresh_ids_from_base() {
        let base = scripted_agent("orchestrator");
        let entry = derive_entry(&base, "-worker-1", vec!["rust".into()]);
        assert_eq!(entry.agent_id(), "orchestrator-worker-1");
        let derived = entry.as_agent().unwrap();
        assert_ne!(Arc::as_ptr(derived), Arc::as_ptr(&base), "distinct agents");
        assert_eq!(derived.id(), "orchestrator-worker-1");

        // Placeholder entries carry no agent but still pool fine.
        let ph = AgentEntry::placeholder("cap-0", vec![]);
        assert!(ph.as_agent().is_none());
        assert!(ph.is_idle());
    }

    // -- acquisition preference ---------------------------------------------

    #[test]
    fn acquire_prefers_specialty_match_over_generic_idle() {
        let reg = pool_with(&[("generic", &[]), ("specialist", &["rust"])]);

        let got = reg.acquire(&["rust"]).unwrap();
        assert_eq!(got.agent_id(), "specialist", "specialist wins while idle");

        // Specialist busy now → fallback picks the generic idle entry.
        let second = reg.acquire(&["rust"]).unwrap();
        assert_eq!(second.agent_id(), "generic");

        // Pool exhausted.
        assert!(reg.acquire(&["rust"]).is_none());
        assert_eq!(
            reg.stats(),
            RegistryStats {
                total: 2,
                idle: 0,
                busy: 2
            }
        );
    }

    #[test]
    fn exhausted_pool_returns_none_not_a_new_agent() {
        let reg = pool_with(&[("only", &["sql"])]);
        assert!(reg.acquire(&["sql"]).is_some());
        assert!(reg.acquire(&["sql"]).is_none());
        assert!(reg.acquire(&[]).is_none());
        // Factory composition (growing the pool) is wave-B's job.
        assert_eq!(reg.stats().idle, 0);
    }

    #[test]
    fn release_makes_entry_reusable_again() {
        let reg = pool_with(&[("worker", &["rust"])]);
        let got = reg.acquire(&["rust"]).unwrap();
        assert!(!got.is_idle());

        // Double-release: second one reports false, no state damage.
        assert!(reg.release(&got));
        assert!(!reg.release(&got));

        assert!(got.is_idle());
        let again = reg.acquire(&["nonexistent-specialty"]).unwrap();
        assert_eq!(again.agent_id(), "worker", "released entry is reusable");
    }

    #[test]
    fn no_specialty_preference_still_acquires_any_idle() {
        let reg = pool_with(&[("x", &["rust"]), ("y", &["python"])]);
        let first = reg.acquire(&[]).unwrap();
        assert_eq!(first.agent_id(), "x"); // registration order
        let second = reg.acquire(&[]).unwrap();
        assert_eq!(second.agent_id(), "y");
    }

    // -- concurrency ----------------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_acquire_release_never_double_books() {
        const TASKS: usize = 64;
        const POOL: usize = 4;

        let reg = Arc::new(pool_with(&[
            ("w0", &["rust"]),
            ("w1", &["rust"]),
            ("w2", &["python"]),
            ("w3", &[]),
        ]));

        // active[id] counts holders right now; any value > 1 = double-booked.
        let mut active: Vec<(String, std::sync::Arc<std::sync::atomic::AtomicUsize>)> = Vec::new();
        for i in 0..POOL {
            active.push((
                format!("w{i}"),
                std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            ));
        }
        let active = Arc::new(active);
        let total_acquires = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let mut joins = Vec::new();
        for t in 0..TASKS {
            let reg = Arc::clone(&reg);
            let active = Arc::clone(&active);
            let total = Arc::clone(&total_acquires);
            joins.push(tokio::spawn(async move {
                let preferred: Vec<&str> = if t % 2 == 0 { vec!["rust"] } else { vec![] };
                // acquire() is non-blocking; contend until we win one.
                let entry = loop {
                    if let Some(e) = reg.acquire(&preferred) {
                        break e;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                };
                {
                    let slot = active
                        .iter()
                        .find(|(id, _)| *id == entry.agent_id())
                        .map(|(_, c)| c)
                        .expect("acquired id must be a pooled id");
                    let prev = slot.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    assert_eq!(prev, 0, "entry {} double-booked!", entry.agent_id());
                    assert!(!entry.is_idle());
                    // Hold across yield points to widen the race window.
                    tokio::task::yield_now().await;
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    slot.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                }
                assert!(reg.release(&entry));
                total.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }));
        }
        for join in joins {
            join.await.unwrap();
        }

        // Every entry released exactly to idle; acquisitions all accounted.
        assert_eq!(
            total_acquires.load(std::sync::atomic::Ordering::SeqCst),
            TASKS
        );
        assert_eq!(
            reg.stats(),
            RegistryStats {
                total: POOL,
                idle: POOL,
                busy: 0
            }
        );
    }
}
