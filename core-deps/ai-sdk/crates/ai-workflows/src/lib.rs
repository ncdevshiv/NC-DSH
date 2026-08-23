//! Workflow engine (spec §15, PRD §3.8.1): sequential steps, parallel
//! branches, conditionals, retries, timeouts, cancellation, and state
//! checkpoints. Nodes compose into a tree; state flows top-down.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use ai_errors::{AiError, WorkflowError};
use ai_runtime::{Parallel, RetryPolicy, Task};

/// A node handler: receives the node input and shared mutable state,
/// returns the node output.
#[async_trait]
pub trait NodeHandler: Send + Sync {
    /// Runs the handler with `input` and the current `state` (by value).
    /// Returns `(output, state)` so state flows through retries cleanly.
    async fn run(
        &self,
        input: serde_json::Value,
        state: serde_json::Value,
    ) -> Result<(serde_json::Value, serde_json::Value), AiError>;
}

/// Handler type for closure-based node handlers.
pub type NodeHandlerFn = dyn Fn(
        serde_json::Value,
        serde_json::Value,
    ) -> Result<(serde_json::Value, serde_json::Value), AiError>
    + Send
    + Sync;

/// A closure-based handler.
pub struct FunctionNodeHandler {
    handler: Box<NodeHandlerFn>,
}

impl FunctionNodeHandler {
    pub fn new(
        handler: impl Fn(
            serde_json::Value,
            serde_json::Value,
        ) -> Result<(serde_json::Value, serde_json::Value), AiError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            handler: Box::new(handler),
        }
    }
}

#[async_trait]
impl NodeHandler for FunctionNodeHandler {
    async fn run(
        &self,
        input: serde_json::Value,
        state: serde_json::Value,
    ) -> Result<(serde_json::Value, serde_json::Value), AiError> {
        (self.handler)(input, state)
    }
}

/// A boxed, Send node-execution future.
pub type BoxedNodeFuture<'a> = Pin<
    Box<dyn Future<Output = Result<(serde_json::Value, serde_json::Value), AiError>> + Send + 'a>,
>;

/// Async handler type for non-blocking node handlers.
pub type AsyncNodeHandlerFn = dyn Fn(
        serde_json::Value,
        serde_json::Value,
    ) -> Pin<
        Box<dyn Future<Output = Result<(serde_json::Value, serde_json::Value), AiError>> + Send>,
    > + Send
    + Sync;

/// A handler wrapping an async closure (useful for non-blocking steps).
pub struct AsyncFunctionNodeHandler {
    handler: Box<AsyncNodeHandlerFn>,
}

impl AsyncFunctionNodeHandler {
    pub fn new(
        handler: impl Fn(
            serde_json::Value,
            serde_json::Value,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<(serde_json::Value, serde_json::Value), AiError>> + Send,
            >,
        > + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            handler: Box::new(handler),
        }
    }
}

#[async_trait]
impl NodeHandler for AsyncFunctionNodeHandler {
    async fn run(
        &self,
        input: serde_json::Value,
        state: serde_json::Value,
    ) -> Result<(serde_json::Value, serde_json::Value), AiError> {
        (self.handler)(input, state).await
    }
}

/// A workflow node.
pub enum Node {
    Step(StepNode),
    Parallel(ParallelNode),
    Conditional(ConditionalNode),
}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Step(node) => write!(f, "Step({})", node.name),
            Self::Parallel(node) => write!(
                f,
                "Parallel({}, {} branches)",
                node.name,
                node.branches.len()
            ),
            Self::Conditional(node) => write!(f, "Conditional({})", node.name),
        }
    }
}

/// A single executable step.
pub struct StepNode {
    pub name: String,
    pub handler: Arc<dyn NodeHandler>,
}

/// A fan-out of branches executed with bounded concurrency; results are
/// collected into an array (fan-in).
pub struct ParallelNode {
    pub name: String,
    pub branches: Vec<Node>,
    pub concurrency: usize,
}

/// A conditional branch.
pub struct ConditionalNode {
    pub name: String,
    pub condition: Arc<dyn Fn(&serde_json::Value) -> bool + Send + Sync>,
    pub then_branch: Box<Node>,
    pub else_branch: Option<Box<Node>>,
}

/// Persists workflow state checkpoints.
#[async_trait]
pub trait CheckpointStore: Send + Sync {
    async fn save(&self, workflow_id: &str, state: &serde_json::Value) -> Result<(), AiError>;
    async fn load(&self, workflow_id: &str) -> Result<Option<serde_json::Value>, AiError>;
}

/// Configuration for a workflow run.
#[derive(Debug, Clone)]
pub struct WorkflowConfig {
    pub retry: RetryPolicy,
    /// Overall deadline (None = no deadline).
    pub timeout: Option<Duration>,
    /// Checkpoint after every step.
    pub checkpoint: bool,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            retry: RetryPolicy::default(),
            timeout: None,
            checkpoint: true,
        }
    }
}

/// Compile-time assertion that workflow types are Send (used to diagnose
/// executor compatibility).
pub fn assert_types_send() {
    fn check<T: Send>() {}
    check::<Workflow>();
    check::<Node>();
    check::<WorkflowConfig>();
    check::<ai_runtime::RetryPolicy>();
}

/// A runnable workflow.
pub struct Workflow {
    pub id: String,
    root: Node,
    config: WorkflowConfig,
    checkpoint_store: Option<Arc<dyn CheckpointStore>>,
}

impl Workflow {
    pub fn new(id: impl Into<String>, root: Node) -> Self {
        Self {
            id: id.into(),
            root,
            config: WorkflowConfig::default(),
            checkpoint_store: None,
        }
    }

    pub fn with_config(mut self, config: WorkflowConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_checkpoint_store(mut self, store: Arc<dyn CheckpointStore>) -> Self {
        self.checkpoint_store = Some(store);
        self
    }

    /// Executes the workflow with `input`, returning the root output.
    /// Cancellation: dropping the returned future aborts in-flight work.
    pub async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, AiError> {
        let state = serde_json::json!({"input": input.clone(), "nodes": {}});
        let run = async {
            let (output, _state) = self
                .run_node_boxed(&self.root, input.clone(), state)
                .await?;
            Ok::<serde_json::Value, AiError>(output)
        };

        match self.config.timeout {
            Some(d) => match tokio::time::timeout(d, run).await {
                Ok(result) => result,
                Err(_) => Err(AiError::Workflow(WorkflowError::new(
                    &self.id,
                    format!("workflow exceeded the {} ms deadline", d.as_millis()),
                ))),
            },
            None => run.await,
        }
    }

    /// Executes a node with `state` owned by the future (state flows by
    /// value so the recursive executor is Send-provable). Returns
    /// `(output, state)`.
    fn run_node_boxed<'a>(
        &'a self,
        node: &'a Node,
        input: serde_json::Value,
        state: serde_json::Value,
    ) -> BoxedNodeFuture<'a> {
        Box::pin(async move {
            let mut state = state;
            let output = match node {
                Node::Step(step) => {
                    // Each retry attempt receives a clone of the pre-step
                    // state (correct retry semantics: a failed attempt's
                    // partial mutations are discarded).
                    let (result, state_after) =
                        ai_runtime::retry(&self.config.retry, &step.name, || {
                            let handler = step.handler.clone();
                            let input = input.clone();
                            let state = state.clone();
                            async move { handler.run(input, state).await }
                        })
                        .await
                        .map_err(|e| {
                            AiError::Workflow(WorkflowError::with_source(
                                &self.id,
                                format!("step `{}` failed", step.name),
                                e,
                            ))
                        })?;
                    state = state_after;
                    if self.config.checkpoint {
                        let mut checkpoint = state.clone();
                        checkpoint["last_step"] = serde_json::json!(step.name);
                        checkpoint["last_output"] = result.clone();
                        if let Some(store) = &self.checkpoint_store {
                            let _ = store.save(&self.id, &checkpoint).await;
                        }
                    }
                    result
                }
                Node::Parallel(parallel) => {
                    let tasks: Vec<Task<(serde_json::Value, serde_json::Value)>> = parallel
                        .branches
                        .iter()
                        .enumerate()
                        .map(|(index, branch)| {
                            let branch_name = format!("{}.{}", parallel.name, index);
                            let branch = clone_node(branch);
                            let input = input.clone();
                            Task::new(branch_name.clone(), async move {
                                let workflow = Self {
                                    id: branch_name.clone(),
                                    root: branch,
                                    config: WorkflowConfig {
                                        checkpoint: false,
                                        ..Default::default()
                                    },
                                    checkpoint_store: None,
                                };
                                workflow
                                    .run_node_boxed(&workflow.root, input, serde_json::Value::Null)
                                    .await
                            })
                        })
                        .collect();
                    let results = Parallel::new()
                        .with_limit(parallel.concurrency.max(1))
                        .execute(tasks)
                        .await;
                    let mut outputs = Vec::with_capacity(results.len());
                    for result in results {
                        let (output, _) = result.outcome.map_err(|e| {
                            AiError::Workflow(WorkflowError::with_source(
                                &self.id,
                                format!("parallel branch `{}` failed", result.name),
                                e,
                            ))
                        })?;
                        outputs.push(output);
                    }
                    serde_json::json!(outputs)
                }
                Node::Conditional(conditional) => {
                    let take_then = (conditional.condition)(&input);
                    let branch = if take_then {
                        &conditional.then_branch
                    } else {
                        conditional.else_branch.as_deref().ok_or_else(|| {
                            AiError::Workflow(WorkflowError::new(
                                &self.id,
                                format!(
                                    "conditional `{}`: no else branch and condition was false",
                                    conditional.name
                                ),
                            ))
                        })?
                    };
                    let (output, state_after) = self.run_node_boxed(branch, input, state).await?;
                    state = state_after;
                    output
                }
            };

            if let Some(nodes) = state.get_mut("nodes").and_then(|n| n.as_object_mut()) {
                nodes.insert(node_name(node), output.clone());
            }
            Ok((output, state))
        })
    }

    /// Resumes from a checkpoint: returns the saved state if present.
    pub async fn load_checkpoint(&self) -> Result<Option<serde_json::Value>, AiError> {
        match &self.checkpoint_store {
            Some(store) => store.load(&self.id).await,
            None => Ok(None),
        }
    }
}

fn node_name(node: &Node) -> String {
    match node {
        Node::Step(step) => step.name.clone(),
        Node::Parallel(parallel) => parallel.name.clone(),
        Node::Conditional(conditional) => conditional.name.clone(),
    }
}

/// Clones a node tree (handlers/conditions are Arc-shared).
fn clone_node(node: &Node) -> Node {
    match node {
        Node::Step(step) => Node::Step(StepNode {
            name: step.name.clone(),
            handler: step.handler.clone(),
        }),
        Node::Parallel(parallel) => Node::Parallel(ParallelNode {
            name: parallel.name.clone(),
            branches: parallel.branches.iter().map(clone_node).collect(),
            concurrency: parallel.concurrency,
        }),
        Node::Conditional(conditional) => Node::Conditional(ConditionalNode {
            name: conditional.name.clone(),
            condition: conditional.condition.clone(),
            then_branch: Box::new(clone_node(&conditional.then_branch)),
            else_branch: conditional
                .else_branch
                .as_deref()
                .map(clone_node)
                .map(Box::new),
        }),
    }
}

/// Builder for workflow nodes with shareable conditions.
pub struct NodeBuilder;

impl NodeBuilder {
    pub fn step(name: impl Into<String>, handler: Arc<dyn NodeHandler>) -> Node {
        Node::Step(StepNode {
            name: name.into(),
            handler,
        })
    }

    pub fn step_fn(
        name: impl Into<String>,
        handler: impl Fn(
            serde_json::Value,
            serde_json::Value,
        ) -> Result<(serde_json::Value, serde_json::Value), AiError>
        + Send
        + Sync
        + 'static,
    ) -> Node {
        Self::step(name, Arc::new(FunctionNodeHandler::new(handler)))
    }

    pub fn step_async(
        name: impl Into<String>,
        handler: impl Fn(
            serde_json::Value,
            serde_json::Value,
        ) -> Pin<
            Box<
                dyn Future<Output = Result<(serde_json::Value, serde_json::Value), AiError>> + Send,
            >,
        > + Send
        + Sync
        + 'static,
    ) -> Node {
        Self::step(name, Arc::new(AsyncFunctionNodeHandler::new(handler)))
    }

    pub fn parallel(name: impl Into<String>, branches: Vec<Node>) -> Node {
        Node::Parallel(ParallelNode {
            name: name.into(),
            branches,
            concurrency: 4,
        })
    }

    pub fn parallel_with_limit(
        name: impl Into<String>,
        branches: Vec<Node>,
        concurrency: usize,
    ) -> Node {
        Node::Parallel(ParallelNode {
            name: name.into(),
            branches,
            concurrency: concurrency.max(1),
        })
    }

    pub fn conditional(
        name: impl Into<String>,
        condition: Arc<dyn Fn(&serde_json::Value) -> bool + Send + Sync>,
        then_branch: Node,
    ) -> Node {
        Self::conditional_with_else(name, condition, then_branch, None)
    }

    pub fn conditional_with_else(
        name: impl Into<String>,
        condition: Arc<dyn Fn(&serde_json::Value) -> bool + Send + Sync>,
        then_branch: Node,
        else_branch: Option<Node>,
    ) -> Node {
        Node::Conditional(ConditionalNode {
            name: name.into(),
            condition,
            then_branch: Box::new(then_branch),
            else_branch: else_branch.map(Box::new),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_errors::NetworkError;

    fn add_step(name: String, delta: f64) -> Node {
        let name_for_error = name.clone();
        NodeBuilder::step_fn(name, move |input, state| {
            let value = input.as_f64().ok_or_else(|| {
                AiError::Workflow(WorkflowError::new(
                    name_for_error.as_str(),
                    "input must be a number",
                ))
            })?;
            Ok((serde_json::json!(value + delta), state))
        })
    }

    #[tokio::test]
    async fn sequential_steps_compose() {
        let workflow = Workflow::new(
            "seq",
            NodeBuilder::step_fn("double", |input, state| {
                Ok((
                    serde_json::json!(input.as_f64().unwrap_or(0.0) * 2.0),
                    state,
                ))
            }),
        );
        let output = workflow.execute(serde_json::json!(21)).await.unwrap();
        assert_eq!(output, serde_json::json!(42.0));
    }

    #[tokio::test]
    async fn conditional_picks_branch() {
        let workflow = Workflow::new(
            "cond",
            NodeBuilder::conditional(
                "positive",
                Arc::new(|input| input.as_f64().unwrap_or(0.0) > 0.0),
                add_step("then_add".into(), 100.0),
            ),
        );
        let then_output = workflow.execute(serde_json::json!(1.0)).await.unwrap();
        assert_eq!(then_output, serde_json::json!(101.0));
    }

    #[tokio::test]
    async fn conditional_else_branch() {
        let workflow = Workflow::new(
            "cond-else",
            NodeBuilder::conditional_with_else(
                "positive",
                Arc::new(|input| input.as_f64().unwrap_or(0.0) > 0.0),
                add_step("then_add".into(), 100.0),
                Some(add_step("else_add".into(), -100.0)),
            ),
        );
        let output = workflow.execute(serde_json::json!(-5.0)).await.unwrap();
        assert_eq!(output, serde_json::json!(-105.0));
    }

    #[tokio::test]
    async fn parallel_branches_fan_in() {
        let workflow = Workflow::new(
            "par",
            NodeBuilder::parallel_with_limit(
                "branches",
                vec![
                    add_step("a".into(), 1.0),
                    add_step("b".into(), 2.0),
                    add_step("c".into(), 3.0),
                ],
                2,
            ),
        );
        let output = workflow.execute(serde_json::json!(10.0)).await.unwrap();
        assert_eq!(output, serde_json::json!([11.0, 12.0, 13.0]));
    }

    #[tokio::test]
    async fn retry_recovers_transient_step_failure() {
        let attempts = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempts_clone = attempts.clone();
        let node = NodeBuilder::step_fn("flaky", move |input, state| {
            let n = attempts_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n < 2 {
                Err(AiError::Network(NetworkError::new("flaky", "transient")))
            } else {
                Ok((input, state))
            }
        });
        let workflow = Workflow::new("retry", node).with_config(WorkflowConfig {
            retry: RetryPolicy::default()
                .with_max_attempts(4)
                .with_base_delay(Duration::from_millis(1))
                .with_jitter(0.0),
            ..Default::default()
        });
        let output = workflow.execute(serde_json::json!("ok")).await.unwrap();
        assert_eq!(output, serde_json::json!("ok"));
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn timeout_aborts_workflow() {
        let node = NodeBuilder::step_async("slow", |_input, state| {
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                Ok((serde_json::json!("done"), state))
            })
        });
        let workflow = Workflow::new("slow-workflow", node).with_config(WorkflowConfig {
            timeout: Some(Duration::from_millis(50)),
            ..Default::default()
        });
        let err = workflow.execute(serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, AiError::Workflow(_)), "{err}");
        assert!(err.to_string().contains("deadline"), "{err}");
    }

    #[tokio::test]
    async fn checkpoints_persist_state() {
        struct MemCheckpoint(parking_lot::Mutex<Option<serde_json::Value>>);
        #[async_trait]
        impl CheckpointStore for MemCheckpoint {
            async fn save(&self, _id: &str, state: &serde_json::Value) -> Result<(), AiError> {
                *self.0.lock() = Some(state.clone());
                Ok(())
            }
            async fn load(&self, _id: &str) -> Result<Option<serde_json::Value>, AiError> {
                Ok(self.0.lock().clone())
            }
        }

        let store: Arc<dyn CheckpointStore> =
            Arc::new(MemCheckpoint(parking_lot::Mutex::new(None)));
        let workflow = Workflow::new("checkpointed", add_step("step1".into(), 1.0))
            .with_checkpoint_store(store.clone());
        workflow.execute(serde_json::json!(1.0)).await.unwrap();
        let checkpoint = workflow
            .load_checkpoint()
            .await
            .unwrap()
            .expect("checkpoint saved");
        assert_eq!(checkpoint["last_step"], "step1");
        assert_eq!(checkpoint["last_output"], serde_json::json!(2.0));
        assert_eq!(checkpoint["input"], serde_json::json!(1.0));
    }
}
