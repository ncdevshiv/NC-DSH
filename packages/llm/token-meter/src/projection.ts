/**
 * Pure client-safe token-projection vocabulary.
 *
 * @module @deepseek-ai/dsh-token-meter/projection
 */

/**
 * Durable cumulative provider usage for a complete session log.
 *
 * The four buckets are disjoint. In particular, reasoning tokens are already
 * included in `outputTokens` and are not accumulated again.
 */
export interface TokenUsageProjection {
  uncachedInputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
}

/**
 * Approximate context occupancy for a status display.
 *
 * The fields, when present, are deliberately NOT one atomic request
 * observation: each is a last-wins record of a different moment. Switching
 * models can therefore pair a fresh capacity with the previous route's
 * pressure until the next request reports usage. This is an intentional trade
 * — the value is a user-facing reference, not a billing or gating input. See
 * the token-meter README for the full rationale.
 */
export interface ContextPressureProjection {
  /**
   * Provider-reported prompt size of the most recent request: uncached input
   * plus cache reads and writes. Response output is excluded, so this does not
   * grow as the current turn streams. Absent until a provider reports usage.
   */
  pressureTokens?: number
  /**
   * What the NEXT request's prompt would cost: {@link pressureTokens} plus the
   * heuristic repricing of everything the surface gained or lost since that
   * sample. Only the delta is estimated, so the figure stays anchored to the
   * provider while still reacting the moment a compaction shadows a span —
   * which `pressureTokens` alone cannot do, since compaction reports no usage
   * of its own. Absent until a provider reports usage.
   */
  projectedTokens?: number
  /** Newest recorded route capacity; absent when no adapter advertised one. */
  contextWindow?: number
}

/**
 * Heuristic composition of the next request's context: what the prompt is
 * made of, not what it costs. All three figures use the meter's fixed
 * density estimate, so they will not sum to the provider-anchored
 * `projectedTokens`: the estimator systematically underprices CJK text and
 * JSON schemas, which is exactly the error the anchoring in
 * {@link ContextPressureProjection.projectedTokens} keeps out of the occupancy
 * figure. Present these as approximations of composition, never as a total.
 */
export interface ContextBreakdownProjection {
  /** Heuristic tokens of the last nonempty surviving system prompt in surface order; 0 when none exists. */
  systemTokens: number
  /** Heuristic tokens of the newest request envelope's tool schemas; 0 before any request. */
  toolsTokens: number
  /** Heuristic tokens of every other visible surface node, including superseded system prompts. */
  messageTokens: number
}

/**
 * Measured provider usage one UTC calendar day of one exact model route
 * contributed to one Session.
 *
 * Counts are DISJOINT, mirroring `TokenUsage`: `uncachedInputTokens` excludes
 * `cacheReadTokens` and `cacheWriteTokens`, and `reasoningTokens` is a subset
 * of `outputTokens` already included there. Billed input is the sum of the
 * three prompt buckets.
 *
 * An optional bucket is present only when every request counted in this entry
 * reported it. Absent therefore means "not every provider disclosed it", never
 * "zero": a partial `cacheReadTokens` cannot be shown as 0 without understating
 * prompt traffic.
 */
export interface RouteUsageEntry {
  /** UTC calendar day the counted requests were logged on, `YYYY-MM-DD`. */
  readonly day: string
  /** Provider route that served the requests. */
  readonly provider: string
  /** Model id the same route served. */
  readonly model: string
  /** Billed request attempts counted on this day and route. */
  readonly requests: number
  /** Sum of uncached prompt input across {@link requests}. */
  readonly uncachedInputTokens: number
  /** Sum of prompt cache reads, present only when every request reported the bucket. */
  readonly cacheReadTokens?: number
  /** Sum of prompt cache writes, present only when every request reported the bucket. */
  readonly cacheWriteTokens?: number
  /** Sum of response output, already including {@link reasoningTokens}. */
  readonly outputTokens: number
  /** Sum of reasoning output, present only when every request reported the bucket. */
  readonly reasoningTokens?: number
  /**
   * Sum of full-call totals: each counted request's provider total, or its
   * exact prompt plus output sum when the provider reported the complete prompt
   * split but no total. Required because a request the meter cannot total this
   * way is not counted at all.
   */
  readonly totalTokens: number
}

declare module '@deepseek-ai/dsh-session-projection/types' {
  interface SessionProjectionMap {
    /** Provider-reported usage accumulated across the complete durable log. */
    tokenUsage: TokenUsageProjection
    /** Newest request pressure paired with the newest known route capacity. */
    contextPressure: ContextPressureProjection
    /** Heuristic system/tools/message composition of the next request. */
    contextBreakdown: ContextBreakdownProjection
    /** Measured per-route usage for every calendar day this Session still retains. */
    routeUsage: readonly RouteUsageEntry[]
  }
}
