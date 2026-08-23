//! The agent runtime: instructions + model + tools + memory with an
//! explicit tool loop, lifecycle states, HITL hooks, and event emission.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use ai_core::{ChatRequest, Model, ToolDefinition};
use ai_errors::{AgentError, AiError, InternalError};
use ai_memory::{InProcessMemory, Memory};
use ai_observability::{
    EventCollector, EventKind, EventSink, EventStatus, ExportError, TraceContext,
};
use ai_runtime::RetryPolicy;
use ai_tools::{Tool, ToolContext, ToolOutput, ToolRegistry, run_tool};
use ai_types::{ContentPart, Message, Role, Usage};

/// The agent's lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Running,
    AwaitingInput,
    Completed,
    Failed,
}

/// The outcome of an agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub text: String,
    pub tool_calls_used: usize,
    pub iterations: u32,
    pub state: AgentState,
    pub usage: Usage,
    /// Tool outputs observed during the run (for debugging/audit).
    pub tool_outputs: Vec<String>,
}

/// Human-in-the-loop hook: consulted before every tool execution.
#[async_trait]
pub trait HumanInTheLoop: Send + Sync {
    /// Legacy boolean gate: `true` executes the tool, `false` rejects it.
    async fn request_confirmation(
        &self,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<bool, AiError>;

    /// Richer decision hook. Defaults to mapping
    /// [`request_confirmation`](Self::request_confirmation) onto
    /// [`HitlDecision::Approve`] / [`HitlDecision::Reject`]; override it to
    /// escalate to a human with [`HitlDecision::Escalate`].
    async fn decide(
        &self,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<HitlDecision, AiError> {
        Ok(HitlDecision::from_approved(
            self.request_confirmation(tool, arguments).await?,
        ))
    }
}

/// No-op HITL hook (all tools auto-approved).
pub struct AutoApprove;

#[async_trait]
impl HumanInTheLoop for AutoApprove {
    async fn request_confirmation(
        &self,
        _tool: &str,
        _arguments: &serde_json::Value,
    ) -> Result<bool, AiError> {
        Ok(true)
    }
}

/// A human-in-the-loop outcome for a pending tool execution.
///
/// The legacy [`HumanInTheLoop::request_confirmation`] hook can only
/// approve or reject; [`HitlDecision::Escalate`] is the additive variant
/// that suspends the run: the agent stops before executing the tool and
/// finishes with [`AgentState::AwaitingInput`] so a human (or another
/// process) can take over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitlDecision {
    /// Execute the tool.
    Approve,
    /// Skip the tool; a rejection marker is fed back to the model as the
    /// tool result and the loop continues.
    Reject,
    /// Suspend the run before executing the tool; the run ends with
    /// [`AgentState::AwaitingInput`].
    Escalate,
}

impl HitlDecision {
    /// Maps the legacy boolean hook outcome onto a decision.
    pub fn from_approved(approved: bool) -> Self {
        if approved {
            Self::Approve
        } else {
            Self::Reject
        }
    }
}

/// Factory minting a fresh conversation store. Used by [`Agent::derive`]
/// (and the swarm engine) so every derived agent owns its own memory
/// instead of sharing one conversation keyed by a cloned id.
pub type MemoryFactory = Arc<dyn Fn() -> Arc<dyn Memory> + Send + Sync>;

/// Default factory: an empty [`InProcessMemory`] per call.
pub fn default_memory_factory() -> MemoryFactory {
    Arc::new(|| Arc::new(InProcessMemory::new(100)))
}

/// Builder for an [`Agent`].
pub struct AgentBuilder {
    id: String,
    instructions: String,
    model: Arc<dyn Model>,
    tools: ToolRegistry,
    memory: Arc<dyn Memory>,
    memory_factory: Option<MemoryFactory>,
    max_iterations: u32,
    retry: RetryPolicy,
    collector: Option<EventCollector>,
    exporters: Vec<Arc<dyn EventSink>>,
    hitl: Arc<dyn HumanInTheLoop>,
    persistent_memory: bool,
}

impl AgentBuilder {
    pub fn new(
        id: impl Into<String>,
        instructions: impl Into<String>,
        model: Arc<dyn Model>,
    ) -> Self {
        Self {
            id: id.into(),
            instructions: instructions.into(),
            model,
            tools: ToolRegistry::new(),
            memory: Arc::new(InProcessMemory::new(100)),
            memory_factory: None,
            max_iterations: 10,
            retry: RetryPolicy::default(),
            collector: None,
            exporters: Vec::new(),
            hitl: Arc::new(AutoApprove),
            persistent_memory: false,
        }
    }

    pub fn with_tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_memory(mut self, memory: Arc<dyn Memory>) -> Self {
        self.memory = memory;
        self
    }

    /// Sets the factory used to mint FRESH memory for every agent produced
    /// by [`Agent::derive`] (and for this builder's own agent, when no
    /// explicit [`with_memory`](Self::with_memory) instance was given).
    ///
    /// Semantics:
    /// - `with_memory(mem)` — `mem` backs exactly the built agent.
    /// - `with_memory_factory(f)` — every derived agent calls `f()` and
    ///   therefore starts with an empty conversation; nothing is shared.
    /// - If both are set, the explicit instance backs the original agent
    ///   while derives use fresh memory from the factory.
    pub fn with_memory_factory(mut self, factory: MemoryFactory) -> Self {
        self.memory_factory = Some(factory);
        self
    }

    /// Opts INTO cross-run conversation persistence: consecutive runs on
    /// the same agent share one history keyed by the agent id. The default
    /// (recommended) is run-scoped memory: each `run()` gets an ephemeral
    /// conversation that is cleared when the run ends, so no state bleeds
    /// between runs of a single instance either.
    pub fn with_persistent_memory(mut self, persistent: bool) -> Self {
        self.persistent_memory = persistent;
        self
    }

    pub fn with_max_iterations(mut self, max_iterations: u32) -> Self {
        self.max_iterations = max_iterations.max(1);
        self
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_collector(mut self, collector: EventCollector) -> Self {
        self.collector = Some(collector);
        self
    }

    /// Attaches a durable event sink. After every completed run, the run's
    /// events are flushed through the sink ([`EventSink::export`]); the
    /// in-memory buffer is cleared only after the sink accepted the batch.
    /// Multiple sinks flush in attachment order.
    pub fn with_exporter(mut self, sink: Arc<dyn EventSink>) -> Self {
        self.exporters.push(sink);
        self
    }

    pub fn with_hitl(mut self, hitl: Arc<dyn HumanInTheLoop>) -> Self {
        self.hitl = hitl;
        self
    }

    pub fn build(self) -> Agent {
        let memory_factory = self.memory_factory.unwrap_or_else(default_memory_factory);
        Agent {
            id: self.id,
            instructions: self.instructions,
            model: self.model,
            tools: self.tools,
            memory: self.memory,
            memory_factory,
            max_iterations: self.max_iterations,
            retry: self.retry,
            collector: self.collector,
            exporters: self.exporters,
            hitl: self.hitl,
            persistent_memory: self.persistent_memory,
        }
    }
}

/// A tool-using agent.
///
/// # Memory isolation
///
/// By default every [`run`](Agent::run) executes against an *ephemeral,
/// run-scoped* conversation: nothing persists after the run ends and no two
/// runs of the same instance share history. Opt into cross-run persistence
/// with [`AgentBuilder::with_persistent_memory`]. Derived agents (see
/// [`Agent::derive`]) always start from a fresh conversation produced by the
/// agent's [`MemoryFactory`], which is what makes per-task swarm isolation
/// possible.
pub struct Agent {
    id: String,
    instructions: String,
    model: Arc<dyn Model>,
    tools: ToolRegistry,
    memory: Arc<dyn Memory>,
    memory_factory: MemoryFactory,
    max_iterations: u32,
    retry: RetryPolicy,
    collector: Option<EventCollector>,
    exporters: Vec<Arc<dyn EventSink>>,
    hitl: Arc<dyn HumanInTheLoop>,
    persistent_memory: bool,
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("id", &self.id)
            .field("instructions", &self.instructions)
            .field("tools", &self.tools.names())
            .field("max_iterations", &self.max_iterations)
            .finish()
    }
}

/// Per-run telemetry: mints ONE [`TraceContext`] per execution so every
/// event of the run shares a single trace id, with spans nested under the
/// run's root span.
struct RunTelemetry {
    collector: Option<EventCollector>,
    trace: TraceContext,
    sinks: Vec<Arc<dyn EventSink>>,
}

/// An opened span awaiting its completion event (`Started` … terminal).
struct OpenSpan {
    kind: EventKind,
    operation: String,
    span_id: String,
    started: Instant,
}

impl RunTelemetry {
    fn new(collector: Option<EventCollector>, sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self {
            collector,
            trace: TraceContext::new(),
            sinks,
        }
    }

    /// Records the run-level lifecycle event on the ROOT span itself (the
    /// first event opens the root span; the last closes it).
    fn record_root_state(&self, operation: impl Into<String>, status: EventStatus) {
        let Some(collector) = &self.collector else {
            return;
        };
        collector.record_with_ids(
            EventKind::AgentState,
            operation,
            status,
            Default::default(),
            self.trace.trace_id().to_string(),
            self.trace.root_span_id().to_string(),
            None,
            None,
        );
    }

    /// Records a one-off event on its own fresh span (child of
    /// `parent_span_id`).
    fn record(
        &self,
        kind: EventKind,
        operation: impl Into<String>,
        status: EventStatus,
        parent_span_id: Option<String>,
    ) {
        let Some(collector) = &self.collector else {
            return;
        };
        collector.record_in_trace(&self.trace, kind, operation, status, parent_span_id, None);
    }

    /// Opens a timed span and emits its `Started` event. Every opened span
    /// is a child of the run's root span; pair every `open_span` with
    /// exactly one [`RunTelemetry::close_span`].
    fn open_span(&self, kind: EventKind, operation: impl Into<String>) -> OpenSpan {
        let span = OpenSpan {
            kind,
            operation: operation.into(),
            span_id: self.trace.new_span_id(),
            started: Instant::now(),
        };
        self.emit_span_event(&span, EventStatus::Started, None);
        span
    }

    /// Records the completion event for an opened span, with its measured
    /// duration.
    fn close_span(&self, span: &OpenSpan, status: EventStatus) {
        let duration_ms = Some(span.started.elapsed().as_millis() as u64);
        self.emit_span_event(span, status, duration_ms);
    }

    fn emit_span_event(&self, span: &OpenSpan, status: EventStatus, duration_ms: Option<u64>) {
        let Some(collector) = &self.collector else {
            return;
        };
        collector.record_with_ids(
            span.kind,
            &span.operation,
            status,
            Default::default(),
            self.trace.trace_id().to_string(),
            span.span_id.clone(),
            Some(self.trace.root_span_id().to_string()),
            duration_ms,
        );
    }

    /// Flushes the run's events durably through every attached sink. The
    /// buffer clears only after all sinks accepted the batch; failures keep
    /// the events buffered and return the error.
    fn flush_events(&self) -> Result<(), ExportError> {
        match self.collector.as_ref() {
            Some(collector) if !self.sinks.is_empty() => collector.try_flush(&self.sinks),
            _ => Ok(()),
        }
    }
}

/// Drains telemetry after a run completes. A successful run whose export
/// fails surfaces the export error (the result is dropped, events stay
/// buffered); a failed run keeps its primary error and only logs the export
/// failure.
fn finish_telemetry(
    telemetry: &RunTelemetry,
    result: Result<AgentResult, AiError>,
) -> Result<AgentResult, AiError> {
    match result {
        Ok(agent_result) => {
            telemetry.flush_events().map_err(|export_error| {
                AiError::Internal(InternalError::new(format!(
                    "event export failed after successful run: {export_error}"
                )))
            })?;
            Ok(agent_result)
        }
        Err(primary) => {
            if let Err(export_error) = telemetry.flush_events() {
                tracing::warn!("event export failed after failed run: {export_error}");
            }
            Err(primary)
        }
    }
}

impl Agent {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Derives a same-configured agent with its OWN id and FRESH memory.
    ///
    /// Shared (cheap `Arc` clones): the model, tool registry, event
    /// collector, exporters, and the HITL hook; policy is cloned. Fresh:
    /// the conversation memory — minted from this agent's
    /// [`MemoryFactory`] — and the id (`{id}{id_suffix}`). The derived
    /// agent inherits the factory, so it can derive further. This is the
    /// foundation for swarm fan-out: one configured agent, N isolated
    /// executors.
    pub fn derive(&self, id_suffix: &str) -> Agent {
        Agent {
            id: format!("{}{id_suffix}", self.id),
            instructions: self.instructions.clone(),
            model: self.model.clone(),
            tools: self.tools.clone(),
            memory: (self.memory_factory)(),
            memory_factory: self.memory_factory.clone(),
            max_iterations: self.max_iterations,
            retry: self.retry.clone(),
            collector: self.collector.clone(),
            exporters: self.exporters.clone(),
            hitl: self.hitl.clone(),
            persistent_memory: false,
        }
    }

    fn model_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.definitions()
    }

    /// Runs the agent on `user_input` with a fresh conversation.
    ///
    /// All events of this run share one trace id (one correlated trace,
    /// spans nested under the run's root span). If an exporter is attached
    /// and fails to persist the batch, this returns an export error even
    /// when the agent succeeded; the events remain buffered for a retry.
    pub async fn run(&self, user_input: &str) -> Result<AgentResult, AiError> {
        let telemetry = RunTelemetry::new(self.collector.clone(), self.exporters.clone());
        telemetry.record_root_state(format!("agent:{}:start", self.id), EventStatus::Started);

        let result = self
            .run_traced(vec![Message::text(Role::User, user_input)], &telemetry)
            .await;

        match &result {
            Ok(r) => telemetry.record_root_state(
                format!(
                    "agent:{}:{}",
                    self.id,
                    match r.state {
                        AgentState::Completed => "completed",
                        AgentState::AwaitingInput => "awaiting_input",
                        AgentState::Failed => "failed",
                        _ => "ended",
                    }
                ),
                if r.state == AgentState::Failed {
                    EventStatus::Failed
                } else {
                    EventStatus::Succeeded
                },
            ),
            Err(_) => {
                telemetry.record_root_state(format!("agent:{}:error", self.id), EventStatus::Failed)
            }
        }
        finish_telemetry(&telemetry, result)
    }

    /// Runs the agent, continuing from `history` (the last message must be
    /// the new user input).
    ///
    /// Like [`Agent::run`], this mints its own trace context: every event
    /// of this call belongs to one trace.
    pub async fn run_with_messages(&self, history: Vec<Message>) -> Result<AgentResult, AiError> {
        let telemetry = RunTelemetry::new(self.collector.clone(), self.exporters.clone());
        let result = self.run_traced(history, &telemetry).await;
        finish_telemetry(&telemetry, result)
    }

    async fn run_traced(
        &self,
        history: Vec<Message>,
        telemetry: &RunTelemetry,
    ) -> Result<AgentResult, AiError> {
        let mut messages = history;
        let mut tool_calls_used = 0usize;
        let mut tool_outputs = Vec::new();
        let mut usage = Usage::default();
        let mut iterations = 0u32;

        // Run-scoped conversation key: unless persistence was explicitly
        // requested, every run owns an ephemeral memory scope that is
        // cleared on exit — two runs of one instance never share history,
        // and nothing bleeds across runs.
        let conversation_key = if self.persistent_memory {
            self.id.clone()
        } else {
            format!("{}#run-{}", self.id, uuid::Uuid::new_v4())
        };

        let system = Message::text(Role::System, &self.instructions);

        // Terminal outcome for the run; memory cleanup runs on every exit
        // path (except suspension — see below).
        enum Outcome {
            Done(AgentResult),
            Error(AiError),
            /// HITL escalation: suspend WITHOUT clearing memory so a later
            /// resume (via `run_with_messages` on the same agent) still has
            /// the pending context available through the agent's store.
            Suspended(AgentResult),
        }

        let outcome = loop {
            iterations += 1;
            if iterations > self.max_iterations {
                break Outcome::Error(AiError::Agent(AgentError::new(
                    &self.id,
                    format!("exceeded max iterations ({})", self.max_iterations),
                )));
            }

            // Assemble the request: system + memory + conversation. Built
            // ONCE and reused verbatim by the retry path so a retried
            // generation sees exactly the same context (system prompt,
            // memory contents, and history).
            let memory_messages = self.memory.retrieve(&conversation_key).await?;
            let mut request_messages = vec![system.clone()];
            request_messages.extend(memory_messages);
            request_messages.extend(messages.clone());

            let request = ChatRequest::new(request_messages)
                .with_tools(self.model_definitions())
                .with_max_tokens(4096);

            let generate_operation = format!("agent:{}:generate", self.id);
            let generate_span = telemetry.open_span(EventKind::ModelCall, &generate_operation);
            let completion = match self.model.generate(request.clone()).await {
                Ok(completion) => completion,
                Err(_err) => {
                    // Self-healing: retry the generation with backoff,
                    // replaying the SAME full request context (system +
                    // memory + conversation) instead of rebuilding it
                    // without memory.
                    telemetry.record(
                        EventKind::Retry,
                        format!("agent:{}:retry", self.id),
                        EventStatus::Retrying,
                        Some(generate_span.span_id.clone()),
                    );
                    let model = self.model.clone();
                    let retried = ai_runtime::retry(
                        &self.retry,
                        &format!("agent:{}:generate", self.id),
                        move || {
                            let request = request.clone();
                            let model = model.clone();
                            async move { model.generate(request).await }
                        },
                    )
                    .await;

                    match retried {
                        Ok(completion) => completion,
                        Err(e) => {
                            telemetry.close_span(&generate_span, EventStatus::Failed);
                            break Outcome::Error(AiError::Agent(AgentError::with_source(
                                &self.id,
                                "generation failed after retries",
                                e,
                            )));
                        }
                    }
                }
            };
            telemetry.close_span(&generate_span, EventStatus::Succeeded);
            // Usage is CUMULATIVE across all model calls of this run (the
            // old code overwrote it each iteration, counting only the last
            // call).
            accumulate_usage(&mut usage, completion.usage);

            // Track the assistant turn.
            let mut assistant_parts = vec![ContentPart::text(&completion.text)];
            for call in &completion.tool_calls {
                assistant_parts.push(ContentPart::tool_call(
                    &call.id,
                    &call.name,
                    &call.arguments,
                ));
            }
            messages.push(Message::new(Role::Assistant, assistant_parts));

            if completion.tool_calls.is_empty() {
                break Outcome::Done(AgentResult {
                    text: messages
                        .iter()
                        .filter(|&m| m.role == Role::Assistant)
                        .map(|m| m.text_content())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    tool_calls_used,
                    iterations,
                    state: AgentState::Completed,
                    usage,
                    tool_outputs,
                });
            }

            // Execute tool calls (with HITL + validation).
            let mut escalate = false;
            let mut hitl_error: Option<AiError> = None;
            'tools: for call in &completion.tool_calls {
                tool_calls_used += 1;

                let tool_operation = format!("tool:{}", call.name);

                // An unknown tool name is a MODEL mistake, not a fatal run
                // error: feed a structured error tool-result back so the
                // model can correct itself on the next iteration.
                let Some(tool) = self.tools.get(&call.name) else {
                    let tool_span = telemetry.open_span(EventKind::ToolCall, &tool_operation);
                    let detail = format!("unknown tool `{}`", call.name);
                    tracing::warn!(agent = %self.id, "{detail}; feeding error back to model");
                    messages.push(Message::new(
                        Role::Tool,
                        vec![ContentPart::tool_result(
                            &call.id,
                            &call.name,
                            serde_json::json!({"error": detail}).to_string(),
                            true,
                        )],
                    ));
                    telemetry.close_span(&tool_span, EventStatus::Failed);
                    continue 'tools;
                };

                let tool_span = telemetry.open_span(EventKind::ToolCall, &tool_operation);
                let arguments: serde_json::Value = serde_json::from_str(&call.arguments)
                    .unwrap_or(serde_json::json!({"raw": call.arguments}));

                let decision = match self.hitl.decide(&call.name, &arguments).await {
                    Ok(decision) => decision,
                    Err(e) => {
                        telemetry.close_span(&tool_span, EventStatus::Failed);
                        hitl_error = Some(e);
                        break 'tools;
                    }
                };
                match decision {
                    HitlDecision::Reject => {
                        messages.push(Message::new(
                            Role::Tool,
                            vec![ContentPart::tool_result(
                                &call.id,
                                &call.name,
                                r#"{"error":"rejected by human"}"#,
                                true,
                            )],
                        ));
                        telemetry.close_span(&tool_span, EventStatus::Cancelled);
                        continue 'tools;
                    }
                    HitlDecision::Escalate => {
                        // Suspend before execution: leave a protocol-valid
                        // marker as the tool result, persist the transcript,
                        // and end the run in AwaitingInput.
                        messages.push(Message::new(
                            Role::Tool,
                            vec![ContentPart::tool_result(
                                &call.id,
                                &call.name,
                                serde_json::json!({
                                    "status": "pending_human_review",
                                    "tool": call.name,
                                })
                                .to_string(),
                                false,
                            )],
                        ));
                        telemetry.close_span(&tool_span, EventStatus::Cancelled);
                        escalate = true;
                        break 'tools;
                    }
                    HitlDecision::Approve => {}
                }

                let context = ToolContext {
                    permissions: ai_security::Permissions::new()
                        .allow("net:http")
                        .allow("fs:read"),
                    execution_id: Some(call.id.clone()),
                    deadline: None,
                    max_response_bytes: Some(16 * 1024),
                };

                let output = match run_tool(tool.as_ref(), arguments, &context).await {
                    Ok(output) => output,
                    Err(e) => ToolOutput::error(e.to_string()),
                };
                tool_outputs.push(output.content.clone());
                messages.push(Message::new(
                    Role::Tool,
                    vec![ContentPart::tool_result(
                        &call.id,
                        &call.name,
                        &output.content,
                        output.is_error,
                    )],
                ));
                telemetry.close_span(
                    &tool_span,
                    if output.is_error {
                        EventStatus::Failed
                    } else {
                        EventStatus::Succeeded
                    },
                );
            }

            // Persist the conversation so far.
            for message in &messages {
                self.memory
                    .store(&conversation_key, message.clone())
                    .await?;
            }
            messages.clear();

            if let Some(e) = hitl_error {
                break Outcome::Error(e);
            }
            if escalate {
                break Outcome::Suspended(AgentResult {
                    text: String::new(),
                    tool_calls_used,
                    iterations,
                    state: AgentState::AwaitingInput,
                    usage,
                    tool_outputs,
                });
            }
        };

        let (result, suspended) = match outcome {
            Outcome::Done(result) => (result, false),
            Outcome::Suspended(result) => (result, true),
            Outcome::Error(e) => {
                // A failed run releases its ephemeral scope too.
                self.clear_run_memory(&conversation_key).await;
                return Err(e);
            }
        };

        // Persist the final exchange (a no-op when the loop already stored
        // and cleared the transcript).
        for message in &messages {
            self.memory
                .store(&conversation_key, message.clone())
                .await?;
        }

        // Non-suspended runs release their ephemeral memory scope; the
        // transcript lives entirely in the returned result.
        if !suspended && !self.persistent_memory {
            self.clear_run_memory(&conversation_key).await;
        }

        Ok(result)
    }

    /// Best-effort cleanup of an ephemeral run scope; failures are logged,
    /// never surfaced (cleanup must not mask the run's own result).
    async fn clear_run_memory(&self, conversation_key: &str) {
        if let Err(e) = self.memory.clear(conversation_key).await {
            tracing::warn!(
                agent = %self.id,
                "failed to clear ephemeral run memory: {e}"
            );
        }
    }

    /// Exposes this agent as a tool (sub-agent pattern): the caller's model
    /// can delegate a task, and the sub-agent runs with its own memory.
    pub fn as_tool(&self) -> Arc<dyn Tool> {
        Arc::new(SubAgentTool {
            agent: Arc::new(SubAgentHandle::new(self)),
        })
    }
}

/// Accumulates per-call token usage into a run total. Optional fields add
/// only when at least one side reports them (`None + None = None`).
pub(crate) fn accumulate_usage(total: &mut Usage, add: Usage) {
    total.input_tokens += add.input_tokens;
    total.output_tokens += add.output_tokens;
    fn add_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
        match (a, b) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        }
    }
    total.reasoning_tokens = add_opt(total.reasoning_tokens, add.reasoning_tokens);
    total.cached_input_tokens = add_opt(total.cached_input_tokens, add.cached_input_tokens);
    total.total_tokens = add_opt(total.total_tokens, add.total_tokens);
}

/// Shared handle so the sub-agent tool can be cloned.
pub struct SubAgentHandle {
    agent: Agent,
}

impl SubAgentHandle {
    fn new(agent: &Agent) -> Self {
        // Sub-agents share configuration (model, tools, observability, HITL)
        // via `derive`, but get their own id and a FRESH conversation memory
        // minted from the parent's factory.
        Self {
            agent: agent.derive("-sub"),
        }
    }
}

/// A tool that delegates to a sub-agent (PRD §3.3 subagent delegation).
pub struct SubAgentTool {
    agent: Arc<SubAgentHandle>,
}

#[async_trait]
impl Tool for SubAgentTool {
    fn name(&self) -> &str {
        "delegate"
    }

    fn description(&self) -> &str {
        "Delegates a task to a sub-agent and returns its final answer"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {"type": "string", "description": "The task for the sub-agent"}
            },
            "required": ["task"]
        })
    }

    async fn execute(
        &self,
        arguments: serde_json::Value,
        _context: &ToolContext,
    ) -> Result<ToolOutput, AiError> {
        let task = arguments
            .get("task")
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                AiError::Tool(ai_errors::ToolError::new("delegate", "missing `task`"))
            })?;
        let result = self.agent.agent.run(task).await?;
        Ok(ToolOutput::ok(result.text))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ai_core::{ChatRequest, Model};
    use ai_models::{ModelCapabilities, ModelInfo};
    use ai_observability::{ExecutionEvent, JsonLinesExporter};
    use ai_types::{Completion, ModelId, ProviderId, StreamEvent};

    /// Deterministic in-memory model for unit tests (ADR-007: unit tests
    /// use mocked LLMs; live tests use the real gateway).
    pub struct ScriptedModel {
        script: Vec<Completion>,
        index: std::sync::Mutex<usize>,
    }

    impl ScriptedModel {
        pub fn new(script: Vec<Completion>) -> Self {
            Self {
                script,
                index: std::sync::Mutex::new(0),
            }
        }
    }

    pub fn completion(text: &str, tool_calls: Vec<ai_types::ToolCall>) -> Completion {
        Completion {
            provider: ProviderId::new("test"),
            model: ModelId::new("scripted"),
            text: text.to_string(),
            tool_calls,
            usage: Usage::new(10, 5),
            reasoning: None,
            raw: serde_json::Value::Null,
            finish_reason: Some("stop".to_string()),
        }
    }

    #[async_trait]
    impl Model for ScriptedModel {
        fn info(&self) -> &ModelInfo {
            static INFO: std::sync::OnceLock<ModelInfo> = std::sync::OnceLock::new();
            INFO.get_or_init(|| {
                ModelInfo::new(
                    ProviderId::new("test"),
                    ModelId::new("scripted"),
                    128_000,
                    8_192,
                )
                .with_capabilities(ModelCapabilities {
                    supports_tools: true,
                    ..Default::default()
                })
            })
        }

        async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
            let mut index = self.index.lock().unwrap();
            let completion = self
                .script
                .get(*index)
                .cloned()
                .unwrap_or_else(|| completion("", vec![]));
            *index += 1;
            Ok(completion)
        }

        async fn stream(&self, request: ChatRequest) -> Result<ai_core::EventStream, AiError> {
            let completion = self.generate(request).await?;
            let events = vec![
                Ok(StreamEvent::TextDelta {
                    delta: completion.text.clone(),
                }),
                Ok(StreamEvent::Completed {
                    finish_reason: completion.finish_reason.clone(),
                }),
            ];
            Ok(Box::pin(futures::stream::iter(events)))
        }
    }

    fn calculator_tool() -> Arc<dyn Tool> {
        Arc::new(ai_tools::FunctionTool::new(
            "calculator",
            "Evaluates an arithmetic expression",
            serde_json::json!({
                "type": "object",
                "properties": {"expression": {"type": "string"}},
                "required": ["expression"]
            }),
            |args| {
                let expression = args["expression"].as_str().unwrap_or("");
                let result = ai_tools::evaluate_expression(expression)
                    .map_err(|e| AiError::Tool(ai_errors::ToolError::new("calculator", e)))?;
                Ok(ToolOutput::ok(result.to_string()))
            },
        ))
    }

    #[tokio::test]
    async fn agent_completes_without_tools() {
        let model = Arc::new(ScriptedModel::new(vec![completion("Hello!", vec![])]));
        let agent = AgentBuilder::new("a1", "Be helpful", model).build();
        let result = agent.run("hi").await.unwrap();
        assert_eq!(result.state, AgentState::Completed);
        assert!(result.text.contains("Hello!"));
        assert_eq!(result.tool_calls_used, 0);
    }

    #[tokio::test]
    async fn agent_executes_tool_loop() {
        let model = Arc::new(ScriptedModel::new(vec![
            completion(
                "",
                vec![ai_types::ToolCall {
                    id: "c1".into(),
                    name: "calculator".into(),
                    arguments: r#"{"expression":"6 * 7"}"#.into(),
                }],
            ),
            completion("The answer is 42.", vec![]),
        ]));
        let mut tools = ToolRegistry::new();
        tools.register(calculator_tool());
        let agent = AgentBuilder::new("a2", "Use tools when needed", model)
            .with_tools(tools)
            .build();
        let result = agent.run("What is 6*7?").await.unwrap();
        assert_eq!(result.tool_calls_used, 1);
        assert!(result.text.contains("42"), "{:?}", result.text);
        assert!(result.tool_outputs.iter().any(|o| o.contains("42")));
    }

    #[tokio::test]
    async fn agent_stops_at_max_iterations() {
        let tool_call = completion(
            "",
            vec![ai_types::ToolCall {
                id: "c1".into(),
                name: "calculator".into(),
                arguments: r#"{"expression":"1+1"}"#.into(),
            }],
        );
        let model = Arc::new(ScriptedModel::new(vec![tool_call.clone(); 3]));
        let mut tools = ToolRegistry::new();
        tools.register(calculator_tool());
        let agent = AgentBuilder::new("a3", "keep calling tools", model)
            .with_tools(tools)
            .with_max_iterations(3)
            .build();
        let err = agent.run("go").await.unwrap_err();
        assert!(err.to_string().contains("max iterations"), "{err}");
    }

    #[tokio::test]
    async fn hitl_rejection_is_recorded_as_tool_result() {
        struct RejectAll;
        #[async_trait]
        impl HumanInTheLoop for RejectAll {
            async fn request_confirmation(
                &self,
                _tool: &str,
                _args: &serde_json::Value,
            ) -> Result<bool, AiError> {
                Ok(false)
            }
        }
        let model = Arc::new(ScriptedModel::new(vec![
            completion(
                "",
                vec![ai_types::ToolCall {
                    id: "c1".into(),
                    name: "calculator".into(),
                    arguments: r#"{"expression":"1+1"}"#.into(),
                }],
            ),
            completion("fine", vec![]),
        ]));
        let mut tools = ToolRegistry::new();
        tools.register(calculator_tool());
        let agent = AgentBuilder::new("a4", "x", model)
            .with_tools(tools)
            .with_hitl(Arc::new(RejectAll))
            .build();
        let result = agent.run("calc").await.unwrap();
        // The model saw the rejection and moved on.
        assert!(result.text.contains("fine"));
    }

    /// One scripted-model run must produce N events sharing ONE trace id,
    /// with a strictly increasing, well-formed span tree.
    #[tokio::test]
    async fn one_run_produces_one_correlated_trace() {
        let collector = EventCollector::new();
        let model = Arc::new(ScriptedModel::new(vec![
            completion(
                "",
                vec![ai_types::ToolCall {
                    id: "c1".into(),
                    name: "calculator".into(),
                    arguments: r#"{"expression":"6 * 7"}"#.into(),
                }],
            ),
            completion("The answer is 42.", vec![]),
        ]));
        let mut tools = ToolRegistry::new();
        tools.register(calculator_tool());
        let agent = AgentBuilder::new("traced", "use tools", model)
            .with_tools(tools)
            .with_collector(collector.clone())
            .build();

        let result = agent.run("What is 6*7?").await.unwrap();
        assert_eq!(result.state, AgentState::Completed);

        let events = collector.events();
        // start + generate pair + tool pair + post-tool generate pair + completed
        assert_eq!(
            events.len(),
            8,
            "expected 8 correlated events, got {}: {:?}",
            events.len(),
            events
                .iter()
                .map(|e| (&e.kind, &e.operation))
                .collect::<Vec<_>>()
        );

        // ONE trace id across the entire run.
        let mut trace_ids = std::collections::HashSet::new();
        for event in &events {
            trace_ids.insert(event.trace_id.clone());
        }
        assert_eq!(
            trace_ids.len(),
            1,
            "every event of one run must share one trace id"
        );

        // Offsets never go backwards.
        let offsets: Vec<u64> = events.iter().map(|e| e.offset_ms).collect();
        assert!(
            offsets.windows(2).all(|pair| pair[0] <= pair[1]),
            "offsets must be non-decreasing: {offsets:?}"
        );

        // Span tree: the first event opens the root span; only root-span
        // events lack a parent; every referenced parent appeared earlier.
        let root_span = events[0].span_id.clone();
        assert!(
            events[0].parent_span_id.is_none(),
            "the first event opens the trace"
        );
        let mut seen_spans = std::collections::HashSet::new();
        for (index, event) in events.iter().enumerate() {
            match &event.parent_span_id {
                None => assert_eq!(
                    event.span_id, root_span,
                    "only root-span events may have no parent (index {index})"
                ),
                Some(parent) => assert!(
                    seen_spans.contains(parent),
                    "parent `{parent}` must appear before its child (index {index})"
                ),
            }
            seen_spans.insert(event.span_id.clone());
        }

        // Spans pair up: Started first, terminal second, durations honest.
        let mut open: std::collections::HashMap<&str, bool> = std::collections::HashMap::new();
        for event in &events {
            let entry = open.entry(event.span_id.as_str()).or_insert(false);
            if event.status == EventStatus::Started {
                assert!(!*entry, "span `{}` started twice", event.span_id);
                *entry = true;
            } else {
                assert!(*entry, "span `{}` finished without starting", event.span_id);
                if event.span_id != root_span.as_str() {
                    // Child spans are timed; the root closes via the
                    // run-level lifecycle event (untimed by design).
                    assert!(
                        event.duration_ms.is_some(),
                        "terminal event carries duration"
                    );
                }
                *entry = false;
            }
        }
        assert!(
            open.values().all(|is_open| !is_open),
            "every span must close within the run"
        );

        // The trace survives grouping: the inspector view sees ONE trace.
        let run_trace_id = events[0].trace_id.clone();
        assert_eq!(collector.trace(&run_trace_id).len(), 8);
    }

    #[tokio::test]
    async fn exporter_writes_jsonl_the_sdk_can_reload() {
        let collector = EventCollector::new();
        let model = Arc::new(ScriptedModel::new(vec![completion("Persisted!", vec![])]));

        let path = std::env::temp_dir().join(format!(
            "ai-sdk-agent-export-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let exporter: Arc<dyn EventSink> =
            Arc::new(JsonLinesExporter::create_file(&path).expect("export file created"));

        let agent = AgentBuilder::new("exported", "be brief", model)
            .with_collector(collector.clone())
            .with_exporter(exporter)
            .build();
        agent.run("hello").await.expect("run succeeds");
        assert!(
            collector.is_empty(),
            "a successful durable export drains the buffer"
        );

        let text = std::fs::read_to_string(&path).expect("SDK wrote the JSONL file");
        std::fs::remove_file(&path).ok();

        let persisted: Vec<ExecutionEvent> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ExecutionEvent::from_jsonl)
            .collect::<Result<Vec<_>, _>>()
            .expect("every line is a valid event");

        // Same count, order, and identity as the in-memory run produced.
        assert_eq!(persisted.len(), 4, "start + generate pair + completed");
        let operations: Vec<&str> = persisted.iter().map(|e| e.operation.as_str()).collect();
        assert_eq!(
            operations,
            vec![
                "agent:exported:start",
                "agent:exported:generate",
                "agent:exported:generate",
                "agent:exported:completed",
            ]
        );
        let mut trace_ids = std::collections::HashSet::new();
        for event in &persisted {
            trace_ids.insert(event.trace_id.clone());
            let parsed = time_like_parse(&event.wall_time)
                .unwrap_or_else(|e| panic!("wall_time `{}` not RFC 3339: {e}", event.wall_time));
            let _ = parsed;
        }
        assert_eq!(trace_ids.len(), 1, "file preserves the single-trace run");

        // Reload through the lossless path: chronology intact.
        let reloaded = EventCollector::new();
        reloaded.load_events(persisted.iter().cloned());
        assert_eq!(reloaded.events(), persisted);
    }

    fn time_like_parse(raw: &str) -> Result<time::OffsetDateTime, String> {
        time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc3339)
            .map_err(|e| e.to_string())
    }

    struct FailingSink;

    impl EventSink for FailingSink {
        fn export(&self, _events: &[ExecutionEvent]) -> Result<(), ExportError> {
            Err(ExportError::new("simulated disk full"))
        }
    }

    #[tokio::test]
    async fn export_failure_is_propagated_and_events_survive() {
        let collector = EventCollector::new();
        let model = Arc::new(ScriptedModel::new(vec![completion("hi", vec![])]));
        let agent = AgentBuilder::new("doomed-export", "x", model)
            .with_collector(collector.clone())
            .with_exporter(Arc::new(FailingSink))
            .build();

        let err = agent
            .run("hello")
            .await
            .expect_err("export failure surfaces");
        assert!(err.to_string().contains("simulated disk full"), "{err}");
        assert_eq!(
            collector.len(),
            4,
            "failed export must NOT destroy the run's events"
        );
        let trace_ids: std::collections::HashSet<_> = collector
            .events()
            .iter()
            .map(|e| e.trace_id.clone())
            .collect();
        assert_eq!(trace_ids.len(), 1, "retained events stay correlated");
    }

    #[tokio::test]
    async fn run_with_messages_also_yields_one_trace() {
        let collector = EventCollector::new();
        let model = Arc::new(ScriptedModel::new(vec![completion("ok", vec![])]));
        let agent = AgentBuilder::new("hist", "x", model)
            .with_collector(collector.clone())
            .build();

        agent
            .run_with_messages(vec![Message::text(Role::User, "hey")])
            .await
            .unwrap();

        let events = collector.events();
        let trace_ids: std::collections::HashSet<_> =
            events.iter().map(|e| e.trace_id.clone()).collect();
        assert_eq!(trace_ids.len(), 1);
        assert_eq!(
            events.len(),
            2,
            "run_with_messages emits the generate pair only"
        );
    }

    // -----------------------------------------------------------------------
    // HERCULES regression tests
    // -----------------------------------------------------------------------

    /// Wraps a model and records every request it receives (for asserting
    /// exactly what context the agent assembled).
    struct RecordingModel {
        inner: Arc<dyn Model>,
        requests: std::sync::Mutex<Vec<ChatRequest>>,
    }

    impl RecordingModel {
        fn new(inner: Arc<dyn Model>) -> Arc<Self> {
            Arc::new(Self {
                inner,
                requests: std::sync::Mutex::new(Vec::new()),
            })
        }

        fn requests(&self) -> Vec<ChatRequest> {
            self.requests.lock().unwrap().clone()
        }

        /// Concatenated text content of every request received so far.
        fn seen_text(&self) -> String {
            self.requests()
                .iter()
                .flat_map(|r| r.messages.iter().map(|m| m.text_content()))
                .collect::<Vec<_>>()
                .join("\n---\n")
        }
    }

    #[async_trait]
    impl Model for RecordingModel {
        fn info(&self) -> &ModelInfo {
            self.inner.info()
        }

        async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
            self.requests.lock().unwrap().push(request.clone());
            self.inner.generate(request).await
        }

        async fn stream(&self, request: ChatRequest) -> Result<ai_core::EventStream, AiError> {
            self.inner.stream(request).await
        }
    }

    /// Fails the FIRST generation, then succeeds; records whether the
    /// retried request contained the expected memory marker.
    struct RetryProbeModel {
        marker: &'static str,
        failed_once: std::sync::atomic::AtomicBool,
        retry_saw_marker: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl Model for RetryProbeModel {
        fn info(&self) -> &ModelInfo {
            static INFO: std::sync::OnceLock<ModelInfo> = std::sync::OnceLock::new();
            INFO.get_or_init(|| {
                ModelInfo::new(
                    ProviderId::new("test"),
                    ModelId::new("retry-probe"),
                    128_000,
                    8_192,
                )
            })
        }

        async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError> {
            if !self
                .failed_once
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(AiError::Network(ai_errors::NetworkError::new(
                    "test",
                    "transient",
                )));
            }
            let saw = request
                .messages
                .iter()
                .any(|m| m.text_content().contains(self.marker));
            self.retry_saw_marker
                .store(saw, std::sync::atomic::Ordering::SeqCst);
            Ok(completion("recovered after retry", vec![]))
        }

        async fn stream(&self, _request: ChatRequest) -> Result<ai_core::EventStream, AiError> {
            unreachable!("stream unused in this test")
        }
    }

    /// Regression: usage must ACCUMULATE across loop iterations instead of
    /// being overwritten by the last call.
    #[tokio::test]
    async fn usage_accumulates_across_iterations() {
        let mut first = completion(
            "",
            vec![ai_types::ToolCall {
                id: "c1".into(),
                name: "calculator".into(),
                arguments: r#"{"expression":"6 * 7"}"#.into(),
            }],
        );
        first.usage = Usage::new(10, 5);
        let mut second = completion(
            "",
            vec![ai_types::ToolCall {
                id: "c2".into(),
                name: "calculator".into(),
                arguments: r#"{"expression":"2 + 2"}"#.into(),
            }],
        );
        second.usage = Usage::new(7, 3);
        let mut third = completion("done", vec![]);
        third.usage = Usage::new(4, 2);

        let model = Arc::new(ScriptedModel::new(vec![first, second, third]));
        let mut tools = ToolRegistry::new();
        tools.register(calculator_tool());
        let agent = AgentBuilder::new("usage-cumulative", "x", model)
            .with_tools(tools)
            .with_retry(RetryPolicy::none())
            .build();

        let result = agent.run("compute").await.unwrap();
        assert_eq!(result.iterations, 3);
        assert_eq!(result.tool_calls_used, 2);
        // 10+7+4 input and 5+3+2 output — the old code reported (4, 2).
        assert_eq!(result.usage.input_tokens, 21);
        assert_eq!(result.usage.output_tokens, 10);
    }

    /// Regression: a retried generation must replay the FULL context
    /// (system + memory + history), not a memory-less rebuild.
    #[tokio::test]
    async fn retry_replays_full_context_including_memory() {
        let memory = Arc::new(InProcessMemory::new(100));
        const MARKER: &str = "MEMORY-MARKER-XYZ";
        memory
            .store(
                "probe",
                Message::text(Role::User, format!("remember {MARKER}")),
            )
            .await
            .unwrap();

        let probe = Arc::new(RetryProbeModel {
            marker: MARKER,
            failed_once: std::sync::atomic::AtomicBool::new(false),
            retry_saw_marker: std::sync::atomic::AtomicBool::new(false),
        });
        let agent = AgentBuilder::new("probe", "x", probe.clone())
            .with_memory(memory)
            .with_persistent_memory(true) // conversation key = "probe"
            .with_retry(RetryPolicy::default().with_base_delay(std::time::Duration::from_millis(1)))
            .build();

        let result = agent.run("continue").await.unwrap();
        assert!(result.text.contains("recovered"));
        assert!(
            probe
                .retry_saw_marker
                .load(std::sync::atomic::Ordering::SeqCst),
            "the retried request must include prior memory content"
        );
    }

    /// Regression: an unknown tool name is fed back as an error
    /// tool-result so the model can recover; the run no longer aborts.
    #[tokio::test]
    async fn unknown_tool_feeds_error_back_and_continues() {
        let model = Arc::new(ScriptedModel::new(vec![
            completion(
                "",
                vec![ai_types::ToolCall {
                    id: "c1".into(),
                    name: "does_not_exist".into(),
                    arguments: "{}".into(),
                }],
            ),
            completion("recovered gracefully", vec![]),
        ]));
        let recording = RecordingModel::new(model);
        let agent = AgentBuilder::new("unknown-tool", "x", recording.clone())
            .with_retry(RetryPolicy::none())
            .build();

        let result = agent.run("try the tool").await.unwrap();
        assert_eq!(result.state, AgentState::Completed);
        assert!(result.text.contains("recovered"));
        // The model's second attempt must have SEEN the error feedback.
        let saw_feedback = recording.requests()[1]
            .messages
            .iter()
            .flat_map(|m| m.parts.iter())
            .any(|part| match part {
                ContentPart::ToolResult { result } => result
                    .output
                    .contains(r#""error":"unknown tool `does_not_exist`""#),
                _ => false,
            });
        assert!(
            saw_feedback,
            "second request must carry the error tool-result"
        );
    }

    /// A HITL hook that ESCALATES suspends the run in AwaitingInput before
    /// executing the tool.
    struct EscalatingHitl;

    #[async_trait]
    impl HumanInTheLoop for EscalatingHitl {
        async fn request_confirmation(
            &self,
            _tool: &str,
            _args: &serde_json::Value,
        ) -> Result<bool, AiError> {
            unreachable!("decide() is overridden; legacy hook must not be called")
        }

        async fn decide(
            &self,
            _tool: &str,
            _args: &serde_json::Value,
        ) -> Result<HitlDecision, AiError> {
            Ok(HitlDecision::Escalate)
        }
    }

    #[tokio::test]
    async fn hitl_escalation_suspends_run_in_awaiting_input() {
        let model = Arc::new(ScriptedModel::new(vec![
            completion(
                "",
                vec![ai_types::ToolCall {
                    id: "c1".into(),
                    name: "calculator".into(),
                    arguments: r#"{"expression":"9 * 9"}"#.into(),
                }],
            ),
            completion("UNREACHABLE", vec![]),
        ]));
        let mut tools = ToolRegistry::new();
        tools.register(calculator_tool());
        let agent = AgentBuilder::new("escalate", "x", model)
            .with_tools(tools)
            .with_hitl(Arc::new(EscalatingHitl))
            .with_retry(RetryPolicy::none())
            .build();

        let result = agent.run("calc").await.unwrap();
        assert_eq!(result.state, AgentState::AwaitingInput);
        assert!(
            !result.text.contains("UNREACHABLE"),
            "the run stops at escalation"
        );
        assert!(
            result.tool_outputs.is_empty(),
            "the escalated tool never executed"
        );
    }

    /// derive(): same configuration, own id, fresh memory.
    #[tokio::test]
    async fn derive_shares_config_but_freshens_memory_and_id() {
        let memory_factory: MemoryFactory = Arc::new(|| Arc::new(InProcessMemory::new(50)));
        let prototype = AgentBuilder::new(
            "proto",
            "instructions here",
            Arc::new(ScriptedModel::new(vec![])),
        )
        .with_memory_factory(memory_factory)
        .build();

        let derived = prototype.derive("-task-7");
        assert_eq!(derived.id(), "proto-task-7");
        // Same instructions/model/tools config via builder equality checks:
        assert_eq!(derived.instructions, prototype.instructions);
        assert!(Arc::ptr_eq(&derived.model, &prototype.model));

        // Memory is fresh per derive: two derives hold DIFFERENT stores.
        let a = prototype.derive("-a");
        let b = prototype.derive("-b");
        a.memory
            .store(
                &format!("{}#run-x", a.id),
                Message::text(Role::User, "solo"),
            )
            .await
            .unwrap();
        assert!(
            b.memory
                .retrieve(&format!("{}#run-y", b.id))
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// Default isolation: consecutive runs of ONE instance share nothing;
    /// the second run's request contains none of the first run's content.
    #[tokio::test]
    async fn runs_do_not_bleed_within_one_instance_by_default() {
        let model = Arc::new(ScriptedModel::new(vec![
            completion("first answer", vec![]),
            completion("second answer", vec![]),
        ]));
        let recording = RecordingModel::new(model);
        let agent = AgentBuilder::new("iso", "x", recording.clone()).build();

        agent.run("FIRST-RUN-SECRET").await.unwrap();
        agent.run("second run").await.unwrap();

        let all_seen = recording.seen_text();
        let last_request = &recording.requests()[1];
        let last_text = last_request
            .messages
            .iter()
            .map(|m| m.text_content())
            .collect::<String>();
        assert!(
            !last_text.contains("FIRST-RUN-SECRET"),
            "cross-run bleed detected: {all_seen}"
        );
    }

    /// Opt-in persistence restores the old cross-run conversation behavior.
    #[tokio::test]
    async fn persistent_memory_opt_in_shares_history_across_runs() {
        let memory = Arc::new(InProcessMemory::new(100));
        let model = Arc::new(ScriptedModel::new(vec![
            completion("one", vec![]),
            completion("two", vec![]),
        ]));
        let recording = RecordingModel::new(model);
        let agent = AgentBuilder::new("persist", "x", recording.clone())
            .with_memory(memory)
            .with_persistent_memory(true)
            .build();

        agent.run("PERSIST-MARKER").await.unwrap();
        agent.run("again").await.unwrap();

        let last_text = recording.requests()[1]
            .messages
            .iter()
            .map(|m| m.text_content())
            .collect::<String>();
        assert!(
            last_text.contains("PERSIST-MARKER"),
            "persistent mode keeps prior-run history visible"
        );
    }
}
