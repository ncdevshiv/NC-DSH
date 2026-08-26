/**
 * Wire DTOs for the `ai-sidecar` protocol (protocol version 1). Field names
 * mirror the Rust serde representation (`snake_case`); the sidecar owns the
 * authoritative definitions in `crates/ai-sidecar`.
 * @module @deepseek-ai/dsh-llm-ai-sdk/types
 */

/** One provider profile on the `configure` payload. */
export interface SidecarProviderProfile {
  /** Resolved bearer key; profiles without one are skipped by the sidecar. */
  api_key?: string
  /** Endpoint base; required for OpenAI-compatible ids without an SDK default. */
  base_url?: string
  /** Wire dialect the route speaks; omission derives from the route id. */
  api?: SidecarApiKind
}

/**
 * Wire dialects the sidecar can serve a route with. `openai-compatible`
 * covers every OpenAI Chat Completions gateway including custom ids;
 * `anthropic` and `google` select the SDK's native adapters.
 */
export type SidecarApiKind = 'openai-compatible' | 'anthropic' | 'google'

/** The same vocabulary on the plugin's public config surface. */
export type RouteApiKind = SidecarApiKind

/** The `model.discover` request parameters: one unsaved endpoint interrogation. */
export interface SidecarDiscoverParams {
  /** Bearer key for this interrogation alone; keyless gateways may omit it. */
  api_key?: string
  /** Endpoint base; required for the OpenAI-compatible dialect. */
  base_url?: string
  /** Wire dialect to interrogate with; omission means OpenAI-compatible. */
  api?: SidecarApiKind
}

/** One model row of the `model.discover` reply (the Rust `ModelInfo` shape). */
export interface SidecarDiscoveredModel {
  id: string
  name?: string
  context_window?: number
  max_output_tokens?: number
  capabilities?: { input_modalities?: readonly string[] }
}

/** The `configure` request parameters. */
export interface SidecarConfigureParams {
  providers: Record<string, SidecarProviderProfile>
  default_provider?: string
}

/** A streamed model event, serde-tagged exactly as `ai_types::StreamEvent`. */
export type SidecarStreamEvent =
  | { type: 'text_delta'; delta: string }
  | { type: 'reasoning_delta'; delta: string }
  | { type: 'tool_call_started'; id: string; name: string }
  | { type: 'tool_call_delta'; id: string; arguments_delta: string }
  | { type: 'tool_call_completed'; call: { id: string; name: string; arguments: string } }
  | {
    type: 'usage_update'
    usage: {
      input_tokens: number
      output_tokens: number
      reasoning_tokens?: number | null
      cached_input_tokens?: number | null
      total_tokens?: number | null
    }
  }
  | { type: 'error'; message: string }
  | { type: 'completed'; finish_reason?: string | null }

/** Terminal notification for one stream. */
export interface SidecarDoneParams {
  stream_id: string
  ok: boolean
  error?: { kind: string; message: string; retryable: boolean }
}
