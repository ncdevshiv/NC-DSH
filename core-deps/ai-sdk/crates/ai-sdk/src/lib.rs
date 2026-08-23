//! The AI SDK — unified public API.
//!
//! This facade re-exports the workspace crates behind a single import so
//! applications can build with `use ai_sdk::*` and access every subsystem:
//! providers, streaming, agents, tools, memory, RAG, workflows, protocols
//! (MCP + A2A), web research, observability, analytics, security, caching,
//! storage, voice, edge, and the CLI commands.

#[allow(deprecated)] // legacy shared-agent fan-out; kept for compatibility
pub use ai_agents::AgentSwarm;
pub use ai_agents::{
    Agent, AgentBuilder, AgentResult, AgentState, AutoApprove, HitlDecision, HumanInTheLoop,
    MemoryFactory,
};
pub use ai_agents::{
    CompetitiveOutcome, CompetitiveScore, JudgeFn, ReduceNode, ReduceOutcome, RoundRecord,
    SwarmEngine, SwarmResult, SwarmTemplate,
};
pub use ai_analytics::{Metric, MetricsRegistry, RateCounter};
pub use ai_cache::model::{CachedModel, RequestCache, install_cache, register_cached};
pub use ai_cache::{SemanticCache, TtlCache, cosine_similarity};
pub use ai_cli::commands;
pub use ai_computer::native::{ComputerTool, NativeComputerClient};
pub use ai_computer::omnichrome::{BrowserTool as RealBrowserTool, OmniChromeClient};
pub use ai_computer::{ComputerError, JsonRpcHttpClient};
pub use ai_config::{Config, ProviderConfig};
pub use ai_core::{
    AiClient, AiClientBuilder, ChatRequest, Model, Provider, ReasoningEffort, ResponseFormat,
    ToolDefinition,
};
pub use ai_devtools::Inspector;
pub use ai_edge::{Capabilities, Runtime, detect_runtime, unsupported};
pub use ai_errors::{AiError, Result};
pub use ai_memory::{
    CompactingMemory, LongTermMemory, Memory, NgramConfig, NgramEmbeddings, SemanticMemory,
    WorkingMemory,
};
pub use ai_models::{ModelCapabilities, ModelInfo, ModelRegistry, Pricing, default_catalog};
pub use ai_observability::{EventCollector, EventKind, EventStatus, ExecutionEvent};
pub use ai_orchestra::{
    Answer, ClarifyVerdict, PendingQuestion, Planner, Question, QuestionMailbox, RunGuard,
    RunHandle, TaskOutcome,
};
pub use ai_protocols::{
    A2AClient, A2AServer, AgentCard, McpClient, McpHttpClient, McpResource, McpServer, McpTool,
    RealtimeConnection, TaskStatus,
};
pub use ai_providers::{create_provider, create_provider_direct, openai_compat};
pub use ai_rag::{
    ChunkingStrategy, ContextAssembler, CorpusStats, HybridStrategy, KeywordReranker, RagPipeline,
    Reranker, bm25_corpus, reciprocal_rank_fusion,
};
pub use ai_runtime::{
    CircuitBreaker, ConcurrencyLimiter, FallbackModel, Parallel, ResiliencePolicy, ResilientModel,
    RetryPolicy, Task, install_fallback_chain, install_resilience,
};
pub use ai_security::{Permissions, PiiDetector, Redactor, UrlPolicy};
pub use ai_storage::{DocumentStore, InMemoryVectorStore, KeyValueStore, SqliteStore, VectorStore};
pub use ai_stream::{collect_completion, collect_text, sse_parse};
pub use ai_tools::{Tool, ToolContext, ToolOutput, ToolRegistry, default_tools, run_tool};
pub use ai_types::{Completion, ContentPart, Message, Role, StreamEvent, Usage};
pub use ai_voice::{
    Audio, BargeIn, DuplexSession, SpeechToText, TextToSpeech, VadDecision, VoiceActivityDetector,
    parse_wav,
};
pub use ai_web::{
    Crawler, DuckDuckGoSearch, NativeResearchBackend, ResearchBackend, RobotsPolicy,
    SearchProvider, WebClient,
};
pub use ai_workflows::{Node, NodeBuilder, NodeHandler, Workflow};

/// Convenience prelude for typical applications.
pub mod prelude {
    pub use ai_agents::{Agent, AgentBuilder};
    pub use ai_core::{AiClient, ChatRequest, Model};
    pub use ai_memory::Memory;
    pub use ai_tools::{Tool, ToolRegistry, default_tools};
    pub use ai_types::{Message, Role, StreamEvent};
}
