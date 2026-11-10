/**
 * Wire vocabulary of the diagnostics snapshot: what the tracing rings expose
 * over `GET /api/diagnostics` and its SSE stream. Client-safe by construction —
 * no Host import — so an agent, a browser, or a test can read the same shape.
 * @module @deepseek-ai/dsh-telemetry-runtime/types
 */

/** One recorded span or lifecycle event, newest last in the ring. */
export interface TelemetryEvent {
  /** Monotonic sequence in this process. */
  readonly seq: number
  /** UTC timestamp. */
  readonly at: string
  /** Category: `model`, `tool`, `turn`, `step`, `client`, `crash`, `plugin`. */
  readonly kind: string
  /** Span or event name (`openai/gpt`, `tool:read_file`, `turn/end`, …). */
  readonly name: string
  /** Total duration in milliseconds for a completed span. */
  readonly durationMs?: number
  /** Milliseconds to the first streamed chunk, for a model call. */
  readonly ttftMs?: number
  /** Whether the operation succeeded. */
  readonly ok?: boolean
  /** Session the operation belongs to. */
  readonly sessionId?: string
  /** Turn index inside the Session. */
  readonly turn?: number
  /** Provider route of a model call. */
  readonly provider?: string
  /** Model id of a model call. */
  readonly model?: string
  /** Short human detail (an error's first line, a status). */
  readonly detail?: string
}

/** Aggregated latency of one span name. */
export interface LatencySummary {
  readonly name: string
  readonly count: number
  readonly errors: number
  readonly p50: number
  readonly p95: number
  readonly max: number
}

/** Per-Session rollup of turns, steps, tools, and tokens. */
export interface SessionSummary {
  readonly sessionId: string
  readonly turns: number
  readonly steps: number
  readonly toolCalls: number
  readonly inputTokens: number
  readonly outputTokens: number
  /** Last recorded activity, UTC. */
  readonly lastAt: string
  /** Model the Session last ran on. */
  readonly provider?: string
  readonly model?: string
}

/** Per-route rollup of one provider/model pair. */
export interface ModelSummary {
  readonly provider: string
  readonly model: string
  readonly calls: number
  readonly errors: number
  readonly ttftP50: number
  readonly ttftP95: number
  readonly durationP50: number
  readonly lastAt: string
}

/** One Loader row and its fiber state. */
export interface LoaderRow {
  readonly id: string
  readonly name: string
  readonly state: string
  readonly error?: string
}

/** One captured crash. */
export interface CrashRecord {
  readonly at: string
  /** Where the crash was caught (`plugin`, `tool`, `model`, `client`). */
  readonly scope: string
  readonly message: string
  /** First frames of the stack, bounded. */
  readonly stack?: string
}

/** One browser-reported span (click → response, render, network). */
export interface ClientSpan {
  readonly at: string
  readonly kind: string
  readonly name: string
  readonly durationMs: number
  readonly ok?: boolean
  readonly detail?: string
}

/** Process facts that explain a slowdown or a restart. */
export interface ProcessSummary {
  readonly pid: number
  readonly node: string
  readonly platform: string
  readonly arch: string
  readonly uptimeMs: number
  readonly rss: number
  readonly heapUsed: number
  readonly heapTotal: number
}

/** One complete diagnostics answer. */
export interface TelemetrySnapshot {
  readonly generatedAt: string
  readonly process: ProcessSummary
  /** Monotonic counters (`model.calls`, `tool.calls`, `turn.completed`, …). */
  readonly counters: Readonly<Record<string, number>>
  readonly latency: readonly LatencySummary[]
  readonly sessions: readonly SessionSummary[]
  readonly models: readonly ModelSummary[]
  readonly loader: readonly LoaderRow[]
  readonly crashes: readonly CrashRecord[]
  readonly events: readonly TelemetryEvent[]
  readonly client: readonly ClientSpan[]
  /** `ok` is false when a reason names a live failure. */
  readonly health: { readonly ok: boolean; readonly reasons: readonly string[] }
}
