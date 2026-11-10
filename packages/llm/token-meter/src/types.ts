/**
 * Public configuration and measurement vocabulary for replay token metering.
 *
 * @module @deepseek-ai/dsh-token-meter/types
 */

import type { TokenUsage } from '@deepseek-ai/dsh-llm'
import type { SessionLogOffset, SessionSeq } from '@deepseek-ai/dsh-session/types'

export type { ContextBreakdownProjection, ContextPressureProjection, RouteUsageEntry, TokenUsageProjection } from './projection.ts'

/** Token-meter plugin configuration; the estimator itself has no settings. */
export interface TokenMeterConfig {
  /**
   * UTC calendar days of per-route measured usage each Session retains.
   *
   * Bounds the persisted `routeUsage` fold state, so a Session's row cannot
   * grow with its lifetime. A Session cannot answer a window reaching past its
   * oldest retained day, so a deployment that displays a longer window must
   * raise this together with the reader's own configured maximum.
   */
  readonly routeUsageRetentionDays?: number
}

/** {@link TokenMeterConfig} after the schema default is applied. */
export type ResolvedTokenMeterConfig = Required<TokenMeterConfig>

/** The baseline from which a signed surface delta produces current pressure. */
export type TokenMeasurementBaseline =
  | { readonly kind: 'none'; readonly tokens: 0 }
  | { readonly kind: 'estimated'; readonly tokens: number }
  | { readonly kind: 'usage'; readonly tokens: number; readonly usage: Readonly<TokenUsage> }

/** Detached immutable request-pressure and surface snapshot at one consumed log revision. */
export interface TokenMeasurement {
  /** Number of durable events consumed; equal to the next unread event seq. */
  readonly logRevision: SessionLogOffset
  /** Provider or heuristic anchor used for this measurement. */
  readonly baseline: TokenMeasurementBaseline
  /** Signed repricing of current surface content relative to the baseline anchor. */
  readonly surfaceDeltaTokens: number
  /** Non-negative current request-and-response pressure. */
  readonly totalTokens: number
  /** Total route-priced request tokens across the current surface; equals the sum of the node prices. */
  readonly surfaceTokens: number
  /** Current surface nodes in positional head-to-tail order. */
  readonly nodes: readonly TokenSurfaceNode[]
}

/** One token-priced node in the current ordered session surface. */
export interface TokenSurfaceNode {
  /** Durable sequence number of the surface event. */
  readonly seq: SessionSeq
  /**
   * Request-pressure tokens for the exact message projected by this node under
   * the measured route: image occurrences carry the route's declared visual
   * price when the routed adapter declares one, and the fixed heuristic
   * otherwise. Trigger, retention, and range selection all read this price.
   */
  readonly tokens: number
  /**
   * Fixed-heuristic tokens for the same message, independent of any route.
   * The shadow-price protocol prices replacements with this value so the O(1)
   * projection fold stays in agreement with its own appends.
   */
  readonly heuristicTokens: number
}
