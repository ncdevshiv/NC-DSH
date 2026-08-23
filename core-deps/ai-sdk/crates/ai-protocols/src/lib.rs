//! Protocols (spec §16): native MCP (Model Context Protocol) client and
//! server, and A2A (Agent-to-Agent) client and server.
//!
//! - [`mcp`] — JSON-RPC 2.0 based MCP over stdio/Streamable-HTTP:
//!   `initialize`, `tools/list`, `tools/call`, `resources/list`,
//!   `resources/read`, `prompts/list`, `prompts/get`.
//! - [`a2a`] — agent cards, tasks, and messages over a JSON endpoint.
//! - [`realtime`] — Realtime voice session events (OpenAI Realtime compatible)
//!   with tolerant server-event parsing.
//! - [`transport`] — `RealtimeConnection` WebSocket transport over
//!   tokio-tungstenite (split sink/stream, task-safe).
//!
//! Nothing here is mocked: the protocol messages are real JSON-RPC / WS wire
//! exchanges, verified by round-trip tests over real pipes/sockets.

pub mod a2a;
pub mod jsonrpc;
pub mod mcp;
pub mod mcp_http;
pub mod realtime;
pub mod transport;
pub mod ui_stream;

pub use a2a::{A2AClient, A2AServer, AgentCard, MessageRole, Task, TaskStatus};
pub use jsonrpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use mcp::{
    ClientInputs, HandlerOutcome, McpClient, McpPrompt, McpResource, McpServer, McpTool,
    McpToolHandler, PromptArgument, ServerOutcome, SubscriptionHandle, ToolAnnotations,
};
pub use mcp_http::{McpHttpClient, serve_http};
pub use realtime::{
    RealtimeClientEvent, RealtimeEventFramer, RealtimeServerEvent, RealtimeSessionConfig,
};
pub use transport::RealtimeConnection;
pub use ui_stream::{UiDataStreamFramer, UiDataStreamPart};

#[cfg(test)]
mod tests {
    // Protocol round-trip tests live in the submodules.
}
