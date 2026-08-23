//! Fan-out/fan-in parallel execution, race, and fallback strategies.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};

use ai_errors::{AiError, TimeoutError};

use crate::Result;
use crate::concurrency::ConcurrencyLimiter;

/// A named task for parallel execution.
pub struct Task<T> {
    /// Logical name (provider/model/tool id) used in results and limits.
    pub name: String,
    /// Optional concurrency key; when set, the task participates in that
    /// key's concurrency budget.
    pub limit_key: Option<String>,
    future: Pin<Box<dyn Future<Output = Result<T>> + Send>>,
}

impl<T> Task<T> {
    pub fn new(
        name: impl Into<String>,
        future: impl Future<Output = Result<T>> + Send + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            limit_key: None,
            future: Box::pin(future),
        }
    }

    /// Pins this task to a concurrency budget (see
    /// [`ConcurrencyLimiter::set_limit`]).
    pub fn with_limit_key(mut self, key: impl Into<String>) -> Self {
        self.limit_key = Some(key.into());
        self
    }
}

/// Per-task outcome, preserving partial results and errors.
#[derive(Debug)]
pub struct ParallelResult<T> {
    pub name: String,
    pub outcome: Result<T>,
}

impl<T> ParallelResult<T> {
    pub fn succeeded(&self) -> bool {
        self.outcome.is_ok()
    }
}

/// A fan-out/fan-in executor.
///
/// Runs tasks with bounded concurrency (global limit or per-key limits),
/// an optional overall deadline, and cancellation propagation: dropping the
/// returned future aborts all in-flight tasks.
#[derive(Debug, Clone, Default)]
pub struct Parallel {
    /// Global concurrency limit (0 = unlimited).
    pub limit: usize,
    /// Overall deadline for the whole batch (None = no deadline).
    pub deadline: Option<Duration>,
    limiter: ConcurrencyLimiter,
}

impl Parallel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Sets a per-key concurrency limit (shared across executions).
    pub fn set_key_limit(&self, key: &str, limit: usize) {
        self.limiter.set_limit(key, limit);
    }

    /// Executes all tasks, collecting per-task results in input order.
    ///
    /// A deadline produces a [`TimeoutError`] for every task still in
    /// flight when it elapses; the in-flight futures are cancelled and
    /// completed tasks keep their results.
    pub async fn execute<T>(&self, tasks: Vec<Task<T>>) -> Vec<ParallelResult<T>>
    where
        T: Send + 'static,
    {
        let names: Vec<String> = tasks.iter().map(|t| t.name.clone()).collect();
        let total = tasks.len();

        // Single source of truth for outcomes: the deadline path must be
        // able to read results even after the runner future is dropped.
        let store = Arc::new(std::sync::Mutex::new(
            (0..total).map(|_| None).collect::<Vec<_>>(),
        ));

        let run = async {
            let mut queue: VecDeque<Task<T>> = VecDeque::from(tasks);

            // Global budget for bounded concurrency (0 = unlimited).
            let global = ConcurrencyLimiter::new();
            if self.limit > 0 {
                global.set_limit("__global__", self.limit);
            }

            let mut in_flight = FuturesUnordered::new();
            let mut next_index = 0usize;

            // Fill the initial batch up to the concurrency limit.
            while next_index < total && (self.limit == 0 || in_flight.len() < self.limit) {
                let task = queue
                    .pop_front()
                    .expect("queue has tasks while next_index < total");
                in_flight.push(wrap_task(
                    task,
                    next_index,
                    global.clone(),
                    self.limiter.clone(),
                ));
                next_index += 1;
            }

            // Drain, topping up as tasks complete.
            while let Some((index, outcome)) = in_flight.next().await {
                store.lock().expect("store lock not poisoned")[index] = Some(outcome);
                if next_index < total {
                    let task = queue
                        .pop_front()
                        .expect("queue has tasks while next_index < total");
                    in_flight.push(wrap_task(
                        task,
                        next_index,
                        global.clone(),
                        self.limiter.clone(),
                    ));
                    next_index += 1;
                }
            }
        };

        let deadline_elapsed = match self.deadline {
            Some(d) => tokio::time::timeout(d, run).await.is_err(),
            None => {
                run.await;
                false
            }
        };

        // Finalize results: completed tasks keep their outcome; tasks still
        // pending when the deadline elapsed get a timeout error.
        let mut store_guard = store.lock().expect("store lock not poisoned");
        let mut results: Vec<ParallelResult<T>> = Vec::with_capacity(total);
        for (index, name) in names.into_iter().enumerate() {
            let outcome = match store_guard[index].take() {
                Some(outcome) => outcome,
                None => {
                    if deadline_elapsed {
                        let d = self.deadline.expect("deadline set when elapsed");
                        Err(AiError::Timeout(TimeoutError::new(&name, d)))
                    } else {
                        Err(AiError::Internal(ai_errors::InternalError::new(format!(
                            "task `{name}` produced no result"
                        ))))
                    }
                }
            };
            results.push(ParallelResult { name, outcome });
        }
        results
    }
}

/// Wraps a task future: applies the key limit (if any) and reports its
/// index alongside the outcome. `global`/`keyed` are cloned in (cheap Arc
/// clones) so the returned future is `'static`.
fn wrap_task<T>(
    task: Task<T>,
    index: usize,
    global: ConcurrencyLimiter,
    keyed: ConcurrencyLimiter,
) -> impl Future<Output = (usize, Result<T>)> + Send + 'static
where
    T: Send + 'static,
{
    let Task {
        name: _name,
        limit_key,
        future,
    } = task;
    async move {
        // Permits must be *held* for the task duration — binding them to
        // variables keeps the budgets enforced.
        let _global_permit = match global.acquire("__global__").await {
            Ok(p) => p,
            Err(e) => return (index, Err(e)),
        };
        let _key_permit = match limit_key.as_deref() {
            Some(key) => match keyed.acquire(key).await {
                Ok(p) => Some(p),
                Err(e) => return (index, Err(e)),
            },
            None => None,
        };
        let outcome = future.await;
        (index, outcome)
    }
}

/// Runs all tasks concurrently and returns the first successful result
/// (`(name, value)`). Cancels the remaining tasks on success; returns the
/// last error when every task fails.
pub async fn race<T>(tasks: Vec<Task<T>>) -> Result<(String, T)>
where
    T: Send + 'static,
{
    let mut in_flight = FuturesUnordered::new();
    for task in tasks {
        let Task { name, future, .. } = task;
        in_flight.push(async move {
            let outcome = future.await;
            (name, outcome)
        });
    }

    let mut last_error: Option<AiError> = None;
    while let Some((name, outcome)) = in_flight.next().await {
        match outcome {
            Ok(value) => return Ok((name, value)),
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        AiError::Internal(ai_errors::InternalError::new(
            "race requires at least one task",
        ))
    }))
}

/// Tries `primary` first; on failure, tries each backup in order until one
/// succeeds. Returns the primary error when all fail.
pub async fn fallback<T>(primary: Task<T>, backups: Vec<Task<T>>) -> Result<T>
where
    T: Send + 'static,
{
    let mut chain = std::iter::once(primary).chain(backups);

    let first = chain
        .next()
        .expect("fallback requires at least a primary task");
    let Task {
        name: _name,
        future,
        ..
    } = first;
    match future.await {
        Ok(value) => Ok(value),
        Err(primary_error) => {
            for task in chain {
                let Task {
                    name: _name,
                    future,
                    ..
                } = task;
                match future.await {
                    Ok(value) => return Ok(value),
                    Err(_) => continue,
                }
            }
            Err(primary_error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use ai_errors::NetworkError;

    async fn ok_value<T: Send + 'static>(v: T) -> Result<T> {
        Ok(v)
    }

    async fn fail_with(msg: &'static str) -> Result<i32> {
        Err(AiError::Network(NetworkError::new("test", msg)))
    }

    #[tokio::test]
    async fn execute_preserves_input_order() {
        let tasks = vec![
            Task::new("a", ok_value(1)),
            Task::new("b", async {
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok(2)
            }),
            Task::new("c", ok_value(3)),
        ];
        let results = Parallel::new().execute(tasks).await;
        let values: Vec<i32> = results
            .iter()
            .map(|r| *r.outcome.as_ref().unwrap())
            .collect();
        assert_eq!(values, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn execute_preserves_partial_failures() {
        let tasks = vec![
            Task::new("ok", ok_value(1)),
            Task::new("bad", fail_with("down")),
            Task::new("ok2", ok_value(3)),
        ];
        let results = Parallel::new().execute(tasks).await;
        assert!(results[0].succeeded());
        assert!(!results[1].succeeded());
        assert!(results[2].succeeded());
    }

    #[tokio::test]
    async fn execute_enforces_global_limit() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for i in 0..12 {
            let active = active.clone();
            let max_active = max_active.clone();
            tasks.push(Task::new(format!("t{i}"), async move {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                max_active.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(i)
            }));
        }
        let results = Parallel::new().with_limit(3).execute(tasks).await;
        assert_eq!(results.len(), 12);
        assert!(
            max_active.load(Ordering::SeqCst) <= 3,
            "concurrency exceeded: {}",
            max_active.load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn deadline_cancels_in_flight_tasks() {
        let tasks = vec![
            Task::new("slow", async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Ok(1i32)
            }),
            Task::new("fast", ok_value(2)),
        ];
        let results = Parallel::new()
            .with_deadline(Duration::from_millis(50))
            .execute(tasks)
            .await;
        assert!(results[0].outcome.is_err());
        assert!(results[1].succeeded());
    }

    #[tokio::test]
    async fn race_returns_first_success() {
        let tasks = vec![
            Task::new("slow", async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok("slow".to_string())
            }),
            Task::new("fast", ok_value("fast".to_string())),
        ];
        let (name, value) = race(tasks).await.unwrap();
        assert_eq!(name, "fast");
        assert_eq!(value, "fast");
    }

    #[tokio::test]
    async fn race_returns_error_when_all_fail() {
        let tasks = vec![
            Task::new("a", fail_with("x")),
            Task::new("b", fail_with("y")),
        ];
        let err = race(tasks).await.unwrap_err();
        // `race` reports the last error observed.
        assert!(err.to_string().contains("y"), "{err}");
    }

    #[tokio::test]
    async fn fallback_uses_backup_on_primary_failure() {
        let primary = Task::new("primary", fail_with("down"));
        let backup = Task::new("backup", ok_value(42));
        let value = fallback(primary, vec![backup]).await.unwrap();
        assert_eq!(value, 42);
    }

    #[tokio::test]
    async fn fallback_returns_primary_error_when_all_fail() {
        let primary = Task::new("primary", fail_with("primary down"));
        let backup = Task::new("backup", fail_with("backup down"));
        let err = fallback(primary, vec![backup]).await.unwrap_err();
        assert!(err.to_string().contains("primary down"), "{err}");
    }

    #[tokio::test]
    async fn key_limits_are_applied() {
        let executor = Parallel::new();
        executor.set_key_limit("shared", 2);
        assert_eq!(
            executor.limiter.limit("shared"),
            2,
            "key limit must be registered"
        );
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for i in 0..10 {
            let active = active.clone();
            let max_active = max_active.clone();
            tasks.push(
                Task::new(format!("t{i}"), async move {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(i)
                })
                .with_limit_key("shared"),
            );
        }
        let results = executor.execute(tasks).await;
        assert_eq!(results.len(), 10);
        assert!(
            max_active.load(Ordering::SeqCst) <= 2,
            "max concurrency was {}",
            max_active.load(Ordering::SeqCst)
        );
    }
}
