/**
 * The `routeUsage` projection unit: measured provider-reported token counts
 * attributed to the exact provider/model route and UTC calendar day that
 * incurred them.
 *
 * Why a fold and not a log scan: the Models surface asks one question across
 * every Session, so a per-call read of every durable log would price a Settings
 * page visit at the whole corpus. This unit instead accrues each contribution
 * once, in the append path, into a state whose size follows the configured
 * retention rather than the Session's lifetime. The cache row is derived data:
 * a cold read rebuilds it from the complete log, so a restart loses nothing the
 * log holds.
 *
 * Counting rule, shared with the `tokenUsage` unit: an Assistant settlement
 * (`assistant/message` or `assistant/attempt`) contributes the usage it reports,
 * and a second report for the same `(turn, step)` slot replaces the first
 * instead of adding to it — `llm/retry-started` closes the slot so a retried
 * attempt, a separately billed request, does add. Every count passes
 * {@link normalizeUsage}, so a provider report the meter cannot prove internally
 * consistent contributes to no figure at all, `requests` included. The series
 * therefore never overstates.
 *
 * Attribution rule: an assembled Assistant message names its own route, and a
 * settlement that names none — a committed-less attempt — is attributed to the
 * route of the newest `request/header` folded so far. A settlement with neither
 * is dropped rather than guessed at.
 *
 * @module @deepseek-ai/dsh-token-meter/route-usage-projection
 */

import { z } from 'zod'
import type { ProjectionDefinition } from '@deepseek-ai/dsh-session-projection'
import type { SessionEvent } from '@deepseek-ai/dsh-session'
import { messageRoute, normalizeUsage } from './turn-usage.ts'
import { usageOf } from './usage-projection.ts'
import type { RouteUsageEntry } from './projection.ts'
// Type-only: activates the `routeUsage` SessionProjectionMap key declaration.
import type {} from './projection.ts'
// Type-only: activates the `llm/retry-started` SessionEventMap merge.
import type {} from '@deepseek-ai/dsh-llm-retry/types'

const DAY_MS = 86_400_000
const count = z.number().int().nonnegative()
const dayKey = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
const routeName = z.string().min(1)

declare module '@deepseek-ai/dsh-session-projection/types' {
  interface SessionProjectionStateMap {
    routeUsage: RouteUsageState
  }
}

/** Retained per-day, per-route totals; a bucket counts only the requests that reported it. */
interface RouteUsageCell {
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

const contributionSchema = z.object({
  day: dayKey,
  provider: routeName,
  model: routeName,
  uncachedInputTokens: count,
  outputTokens: count,
  totalTokens: count,
  cacheReadTokens: count.optional(),
  cacheWriteTokens: count.optional(),
  reasoningTokens: count.optional(),
}).strict().transform(({ cacheReadTokens, cacheWriteTokens, reasoningTokens, ...rest }) => ({
  ...rest,
  ...cacheReadTokens === undefined ? {} : { cacheReadTokens },
  ...cacheWriteTokens === undefined ? {} : { cacheWriteTokens },
  ...reasoningTokens === undefined ? {} : { reasoningTokens },
}))

const cellSchema = z.object({
  requests: count,
  uncachedInputTokens: count,
  outputTokens: count,
  totalTokens: count,
  cacheReadTokens: count,
  cacheWriteTokens: count,
  reasoningTokens: count,
  cacheReadReports: count,
  cacheWriteReports: count,
  reasoningReports: count,
}).strict()

const entrySchema = z.object({
  day: dayKey,
  provider: routeName,
  model: routeName,
  requests: count,
  uncachedInputTokens: count,
  outputTokens: count,
  totalTokens: count,
  cacheReadTokens: count.optional(),
  cacheWriteTokens: count.optional(),
  reasoningTokens: count.optional(),
}).strict().transform(({ cacheReadTokens, cacheWriteTokens, reasoningTokens, ...rest }) => ({
  ...rest,
  ...cacheReadTokens === undefined ? {} : { cacheReadTokens },
  ...cacheWriteTokens === undefined ? {} : { cacheWriteTokens },
  ...reasoningTokens === undefined ? {} : { reasoningTokens },
}))

const routeUsageStateSchema = z.object({
  /** Route of the newest `request/header`, consulted only for settlements naming no route. */
  route: z.object({ provider: routeName, model: routeName }).nullable(),
  /** Newest retained day; `null` before the first contribution. */
  newestDay: dayKey.nullable(),
  /** Day, then provider, then model. Keys are provider-supplied, so every read is an own-key read. */
  days: z.record(dayKey, z.record(routeName, z.record(routeName, cellSchema))),
  /** The last counted slot, replaced when the same `(turn, step)` settles again. */
  last: z.object({ turn: count, step: count, contribution: contributionSchema }).nullable(),
  /** The wire value, rebuilt only by a fold that changed a retained cell. */
  entries: z.array(entrySchema),
}).strict()

/** The fold state, inferred from its schema so `register` infers the unit's key and state together. */
type RouteUsageState = z.infer<typeof routeUsageStateSchema>

/** One counted request's proven contribution to one day-and-route cell. */
type RetainedContribution = z.infer<typeof contributionSchema>

/**
 * The UTC calendar day one event time falls on, as `YYYY-MM-DD`.
 * @param time - Unix epoch milliseconds recorded on the durable event.
 * @returns the day key, or `null` for a time the meter cannot place on a day.
 */
export function dayKeyOf(time: number): string | null {
  if (!Number.isSafeInteger(time) || time < 0) return null
  return new Date(Math.floor(time / DAY_MS) * DAY_MS).toISOString().slice(0, 10)
}

/** Shift one `YYYY-MM-DD` key by whole UTC days, for the retention cutoff. */
function shiftDay(day: string, delta: number): string {
  const [year, month, date] = day.split('-')
  return new Date(Date.UTC(Number(year), Number(month) - 1, Number(date) + delta)).toISOString().slice(0, 10)
}

/** The route one settlement names, falling back to the newest logged request route. */
function routeOf(event: SessionEvent, logged: RouteUsageState['route']): { provider: string; model: string } | undefined {
  if (event.type === 'assistant/message') {
    const named = messageRoute(event.data.message)
    if (named !== undefined) return named
  }
  return logged ?? undefined
}

/**
 * Add or subtract one contribution from its cell, creating the cell on demand.
 *
 * A subtraction never meets an absent cell while the fold owns `last`, and an
 * absent cell would produce negative counts the nonnegativity schemas reject at
 * the wire and cache boundaries — the loud failure a corrupt derived row
 * deserves, in place of a silent zero.
 */
function applyContribution(
  days: RouteUsageState['days'],
  contribution: RetainedContribution,
  sign: 1 | -1,
): void {
  const providers = days[contribution.day] ?? {}
  const models = providers[contribution.provider] ?? {}
  const cell: RouteUsageCell = models[contribution.model] ?? {
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
  models[contribution.model] = cell
  providers[contribution.provider] = models
  days[contribution.day] = providers

  cell.requests += sign
  cell.uncachedInputTokens += sign * contribution.uncachedInputTokens
  cell.outputTokens += sign * contribution.outputTokens
  cell.totalTokens += sign * contribution.totalTokens
  if (contribution.cacheReadTokens !== undefined) {
    cell.cacheReadTokens += sign * contribution.cacheReadTokens
    cell.cacheReadReports += sign
  }
  if (contribution.cacheWriteTokens !== undefined) {
    cell.cacheWriteTokens += sign * contribution.cacheWriteTokens
    cell.cacheWriteReports += sign
  }
  if (contribution.reasoningTokens !== undefined) {
    cell.reasoningTokens += sign * contribution.reasoningTokens
    cell.reasoningReports += sign
  }
  if (cell.requests === 0) {
    delete models[contribution.model]
    if (Object.keys(models).length === 0) delete providers[contribution.provider]
  }
  if (Object.keys(providers).length === 0) delete days[contribution.day]
}

/** One retained cell's wire entry: an optional bucket appears only when every request reported it. */
function entryOf(day: string, provider: string, model: string, cell: RouteUsageCell): RouteUsageEntry {
  return {
    day,
    provider,
    model,
    requests: cell.requests,
    uncachedInputTokens: cell.uncachedInputTokens,
    outputTokens: cell.outputTokens,
    totalTokens: cell.totalTokens,
    ...cell.cacheReadReports === cell.requests ? { cacheReadTokens: cell.cacheReadTokens } : {},
    ...cell.cacheWriteReports === cell.requests ? { cacheWriteTokens: cell.cacheWriteTokens } : {},
    ...cell.reasoningReports === cell.requests ? { reasoningTokens: cell.reasoningTokens } : {},
  }
}

/** Every retained cell, in day then provider then model order. */
function buildEntries(days: RouteUsageState['days']): RouteUsageEntry[] {
  const entries: RouteUsageEntry[] = []
  for (const [day, providers] of Object.entries(days).toSorted((a, b) => a[0].localeCompare(b[0]))) {
    for (const [provider, models] of Object.entries(providers).toSorted((a, b) => a[0].localeCompare(b[0]))) {
      for (const [model, cell] of Object.entries(models).toSorted((a, b) => a[0].localeCompare(b[0]))) {
        entries.push(entryOf(day, provider, model, cell))
      }
    }
  }
  return entries
}

/** The proven counts, day and route one Assistant settlement contributes, or nothing. */
function contributionOf(
  event: SessionEvent,
  logged: RouteUsageState['route'],
): RetainedContribution | undefined {
  const usage = usageOf(event)
  const day = dayKeyOf(event.time)
  const route = routeOf(event, logged)
  if (usage === undefined || day === null || route === undefined) return undefined
  const attempt = normalizeUsage(usage, route)
  if (attempt === undefined) return undefined
  return {
    day,
    provider: route.provider,
    model: route.model,
    uncachedInputTokens: attempt.inputTokens,
    outputTokens: attempt.outputTokens,
    totalTokens: attempt.totalTokens,
    ...attempt.cacheReadTokens === undefined ? {} : { cacheReadTokens: attempt.cacheReadTokens },
    ...attempt.cacheWriteTokens === undefined ? {} : { cacheWriteTokens: attempt.cacheWriteTokens },
    ...attempt.reasoningTokens === undefined ? {} : { reasoningTokens: attempt.reasoningTokens },
  }
}

/**
 * The `routeUsage` projection unit with its client view required: the unit is
 * registered with a wire projection, so the view is part of the definition.
 */
type RouteUsageProjectionDefinition = ProjectionDefinition<'routeUsage', RouteUsageState>
  & Required<Pick<ProjectionDefinition<'routeUsage', RouteUsageState>, 'wire'>>

/**
 * Build the `routeUsage` unit for one retention.
 *
 * Retention is a deployment choice, so the mounting plugin supplies it from its
 * validated `Config`. The fold prunes by whole UTC days relative to the newest
 * day it has folded, never by wall-clock, so pruning replays identically.
 * @param retentionDays - UTC calendar days each Session keeps, newest first.
 * @returns the projection definition to register on `ctx.sessionProjections`.
 */
export function createRouteUsageProjection(retentionDays: number): RouteUsageProjectionDefinition {
  return {
    key: 'routeUsage',
    stateVersion: 1,
    stateSchema: routeUsageStateSchema,
    init: () => ({ route: null, newestDay: null, days: {}, last: null, entries: [] }),
    apply: (state, event) => {
      switch (event.type) {
        case 'request/header': {
          const { provider, model } = event.data.header.config
          if (provider.length === 0 || model.length === 0) return state
          if (state.route?.provider === provider && state.route.model === model) return state
          return { ...state, route: { provider, model } }
        }
        case 'llm/retry-started': {
          const last = state.last
          if (last === null || last.turn !== event.data.turn || last.step !== event.data.step) return state
          return { ...state, last: null }
        }
        case 'assistant/message':
        case 'assistant/attempt': {
          const contribution = contributionOf(event, state.route)
          if (contribution === undefined) return state
          const days = structuredClone(state.days)
          const previous = state.last
          if (previous !== null && previous.turn === event.data.turn && previous.step === event.data.step) {
            applyContribution(days, previous.contribution, -1)
          }
          applyContribution(days, contribution, 1)
          const newestDay = state.newestDay === null || contribution.day > state.newestDay
            ? contribution.day
            : state.newestDay
          const cutoff = shiftDay(newestDay, 1 - retentionDays)
          for (const day of Object.keys(days)) {
            if (day < cutoff) delete days[day]
          }
          return {
            ...state,
            days,
            newestDay,
            last: contribution.day < cutoff ? null : { turn: event.data.turn, step: event.data.step, contribution },
            entries: buildEntries(days),
          }
        }
        default:
          return state
      }
    },
    wire: { viewSchema: z.array(entrySchema), view: state => state.entries },
  } satisfies ProjectionDefinition<'routeUsage', RouteUsageState>
}
