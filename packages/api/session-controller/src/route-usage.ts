/**
 * Cross-Session windowed read of the harness's own measured per-route token
 * accounting, projected for the Settings → Models surface.
 *
 * Ownership: the measured fact — which request reported which counts, under
 * which route, on which day, and whether a bucket is disclosable — belongs to
 * token-meter's `routeUsage` projection unit, folded once per event from the
 * durable Session log. This module owns only the corpus walk and the window,
 * because it already owns both for the same page: `ApiSessionList.list` walks
 * the identical visible corpus and `buildModelCatalog` projects this page's
 * other provider-shaped answer. Putting the read in the LLM runtime instead
 * would make `packages/llm` depend on Session persistence, and a second
 * aggregate service would duplicate a corpus walk that already exists here.
 *
 * Cost: one already-materialized projection row read per visible Session and
 * no Session log open, so a cold Session never costs a persistence read here.
 * The number of rows is capped by `maxSessions` and the window by
 * `maxWindowDays`; every Session the fold could not see is disclosed by
 * {@link SessionRouteUsageValue.sessionsSkipped} rather than silently omitted.
 * A row older than the reader's window cannot exist: token-meter prunes to its
 * configured retention, so a deployment widening `maxWindowDays` must widen
 * `routeUsageRetentionDays` with it.
 */

import { RemoteError } from '@deepseek-ai/dsh-typert-protocol'
import type { Context } from '@deepseek-ai/cordis'
import type { SessionHeader } from '@deepseek-ai/dsh-session'
// Type-only: activates the `routeUsage` SessionProjectionMap key declaration.
import type {} from '@deepseek-ai/dsh-token-meter'
import type { RouteUsageEntry } from '@deepseek-ai/dsh-token-meter'
import type {
  SessionRouteUsageEntry,
  SessionRouteUsageRequest,
  SessionRouteUsageValue,
} from './types.ts'

const DAY_MS = 86_400_000

/** Bounds the caller may not exceed; resolved from the controller's `Config`. */
export interface RouteUsageLimits {
  /** Largest whole-day window a request may ask for. */
  readonly maxWindowDays: number
  /** Largest number of Session rows one read folds. */
  readonly maxSessions: number
}

/** Per-route totals under construction; a bucket counts only the entries that disclosed it. */
interface RouteAccumulator {
  provider: string
  model: string
  requests: number
  uncachedInputTokens: number
  outputTokens: number
  totalTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
  reasoningTokens: number
  cacheReadReports: number
  cacheWriteReports: number
  reasoningReports: number
}

/** Unix milliseconds of the UTC midnight starting the day a time falls in. */
function utcDayStart(time: number): number {
  return Math.floor(time / DAY_MS) * DAY_MS
}

/** Unix milliseconds of the UTC midnight starting a `YYYY-MM-DD` bucket. */
function dayStart(day: string): number {
  const [year, month, date] = day.split('-')
  return Date.UTC(Number(year), Number(month) - 1, Number(date))
}

/**
 * Validate one window request against the deployment bounds.
 * @param request - caller's day count and optional end time.
 * @param limits - resolved controller bounds.
 * @returns the inclusive and exclusive window bounds in Unix milliseconds.
 * @throws RemoteError with `session/usage-window-invalid` for a request outside the bounds.
 */
export function resolveUsageWindow(
  request: SessionRouteUsageRequest,
  limits: RouteUsageLimits,
): { readonly windowStart: number; readonly windowEnd: number } {
  const { days } = request
  const invalid = (reason: string): RemoteError => new RemoteError(
    'session/usage-window-invalid',
    reason,
    { days, maximum: limits.maxWindowDays },
  )
  if (!Number.isSafeInteger(days) || days < 1) throw invalid('days must be a positive whole number of UTC days')
  if (days > limits.maxWindowDays) throw invalid(`days must not exceed ${String(limits.maxWindowDays)}`)
  const until = request.until
  if (until !== undefined && (!Number.isSafeInteger(until) || until < 0)) {
    throw invalid('until must be a non-negative Unix millisecond timestamp')
  }
  const windowEnd = until ?? Date.now()
  return { windowStart: utcDayStart(windowEnd) - (days - 1) * DAY_MS, windowEnd }
}

/**
 * One Session's retained measured series.
 *
 * A live Session reads the registry's already-materialized cell; a cold one
 * reads the persisted checkpoint row through its listing header, the same two
 * faces `ApiSessionList` uses. `undefined` means no source serves a series yet:
 * no checkpoint row, an unrelated lifecycle row, or a deployment without
 * token-meter or the projection cache.
 */
function seriesOf(ctx: Context, meta: SessionHeader): readonly RouteUsageEntry[] | undefined {
  const live = ctx.sessions.get(meta.id)
  if (live !== undefined) return ctx.sessionProjections.snapshot(live, ['routeUsage']).values.routeUsage
  return ctx.get('sessionProjectionCache')?.cachedSnapshot(meta, ['routeUsage'])?.values.routeUsage
}

/** Add one disclosed day-and-route bucket to its route accumulator. */
function accumulate(target: RouteAccumulator, entry: RouteUsageEntry): void {
  target.requests += entry.requests
  target.uncachedInputTokens += entry.uncachedInputTokens
  target.outputTokens += entry.outputTokens
  target.totalTokens += entry.totalTokens
  if (entry.cacheReadTokens !== undefined) {
    target.cacheReadTokens += entry.cacheReadTokens
    target.cacheReadReports += 1
  }
  if (entry.cacheWriteTokens !== undefined) {
    target.cacheWriteTokens += entry.cacheWriteTokens
    target.cacheWriteReports += 1
  }
  if (entry.reasoningTokens !== undefined) {
    target.reasoningTokens += entry.reasoningTokens
    target.reasoningReports += 1
  }
}

/** Close one accumulator: an optional bucket appears only when every source bucket reported it. */
function finish(target: RouteAccumulator): SessionRouteUsageEntry {
  const { provider, model, requests, uncachedInputTokens, outputTokens, totalTokens } = target
  return {
    provider,
    model,
    requests,
    uncachedInputTokens,
    outputTokens,
    totalTokens,
    ...target.cacheReadReports === requests ? { cacheReadTokens: target.cacheReadTokens } : {},
    ...target.cacheWriteReports === requests ? { cacheWriteTokens: target.cacheWriteTokens } : {},
    ...target.reasoningReports === requests ? { reasoningTokens: target.reasoningTokens } : {},
  }
}

/**
 * Fold every visible Session's measured series into one windowed per-route answer.
 * @param ctx - Host context carrying the Session corpus, registry, and projection cache.
 * @param request - window the caller asked for.
 * @param limits - resolved controller bounds for the window and the row count.
 * @param signal - cancellation for the corpus listing.
 * @returns measured usage for the window, with the Sessions it could not see disclosed.
 * @throws RemoteError with `session/usage-window-invalid` for a request outside `limits`.
 */
export async function buildRouteUsage(
  ctx: Context,
  request: SessionRouteUsageRequest,
  limits: RouteUsageLimits,
  signal: AbortSignal,
): Promise<SessionRouteUsageValue> {
  const { windowStart, windowEnd } = resolveUsageWindow(request, limits)
  const newestDayStart = utcDayStart(windowEnd)
  signal.throwIfAborted()
  const visible = (await ctx.sessionQuery.listSessions(signal))
    .filter(record => ctx.remoteAccess.allows({ kind: 'session', id: record.header.id }))
  const routes = new Map<string, RouteAccumulator>()
  let sessionsCounted = 0
  let sessionsSkipped = Math.max(0, visible.length - limits.maxSessions)
  for (const record of visible.slice(0, limits.maxSessions)) {
    let series: readonly RouteUsageEntry[] | undefined
    try {
      series = seriesOf(ctx, record.header)
    } catch (error: unknown) {
      ctx.logger.warn(`api-session.routeUsage: series for "${record.header.id}" failed; the Session is reported unread: ${String(error)}`)
      series = undefined
    }
    if (series === undefined) {
      sessionsSkipped += 1
      continue
    }
    sessionsCounted += 1
    for (const entry of series) {
      const start = dayStart(entry.day)
      if (start < windowStart || start > newestDayStart) continue
      const key = `${entry.provider}\u0000${entry.model}`
      const target = routes.get(key) ?? {
        provider: entry.provider,
        model: entry.model,
        requests: 0,
        uncachedInputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        cacheReadTokens: 0,
        cacheWriteTokens: 0,
        reasoningTokens: 0,
        cacheReadReports: 0,
        cacheWriteReports: 0,
        reasoningReports: 0,
      }
      accumulate(target, entry)
      routes.set(key, target)
    }
  }
  return {
    windowStart,
    windowEnd,
    routes: [...routes.values()]
      .map(finish)
      .toSorted((left, right) => left.provider.localeCompare(right.provider) || left.model.localeCompare(right.model)),
    sessionsCounted,
    sessionsSkipped,
  }
}
