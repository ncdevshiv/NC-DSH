/**
 * The `routeUsage` projection unit: per-day, per-route measured totals, the
 * disjoint cache-bucket arithmetic, the `(turn, step)` replacement slot,
 * retention pruning, and the disclosure rule that omits a bucket a request did
 * not report.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import { createMessage } from '@deepseek-ai/dsh-llm'
import type { TokenUsage } from '@deepseek-ai/dsh-llm'
import SessionStore from '@deepseek-ai/dsh-session'
import type { Session } from '@deepseek-ai/dsh-session'
import SessionProjectionRegistry from '@deepseek-ai/dsh-session-projection'
import TokenMeter from '@deepseek-ai/dsh-token-meter'
import { RetryId } from '@deepseek-ai/dsh-llm-retry'
import type { RouteUsageEntry } from '@deepseek-ai/dsh-token-meter/client'
import { createRouteUsageProjection, dayKeyOf } from '../src/route-usage-projection.ts'

const DAY = 86_400_000
const FIRST_DAY = Date.UTC(2026, 2, 5)
const FIRST_DAY_KEY = '2026-03-05'

/** `Session.append` stamps `Date.now()`, the only source of a durable event time. */
let clock = FIRST_DAY

beforeEach(() => {
  clock = FIRST_DAY
  vi.spyOn(Date, 'now').mockImplementation(() => clock)
})

afterEach(() => {
  vi.restoreAllMocks()
})

async function harness(retentionDays = 3): Promise<{ ctx: Context; session: Session }> {
  const ctx = new Context()
  await ctx.plugin(SessionStore)
  await ctx.plugin(SessionProjectionRegistry)
  await ctx.plugin(TokenMeter, { routeUsageRetentionDays: retentionDays })
  return { ctx, session: ctx.sessions.create() }
}

function entries(ctx: Context, session: Session): readonly RouteUsageEntry[] {
  const value = ctx.sessionProjections.snapshot(session).values.routeUsage
  if (value === undefined) throw new Error('routeUsage projection is not registered')
  return value
}

function header(session: Session, provider: string, model: string): void {
  session.append('request/header', { header: { config: { provider, model } }, reason: 'initial' })
}

function retryStarted(session: Session, turn: number, step: number, retry: number): void {
  session.append('llm/retry-started', { retryId: RetryId(`r${String(retry)}`), turn, step, retry })
}

/** Append one settled assistant message reporting `usage` under its own route. */
function settled(
  session: Session,
  turn: number,
  step: number,
  usage: TokenUsage,
  route: { provider: string; model: string } = { provider: 'alpha', model: 'one' },
): void {
  session.append('step/start', { turn, step })
  session.append('assistant/message', {
    stream: [{ type: 'chunk', time: 0, chunk: { type: 'usage', usage } }],
    turn,
    step,
    message: createMessage({ role: 'assistant', content: [], source: { kind: 'model', ...route } }),
    usage,
  }, { surfaceOp: 'append' })
  session.append('step/end', { turn, step })
}

/** Append one attempt that committed no message; its route comes from the logged header. */
function attempt(session: Session, turn: number, step: number, usage: TokenUsage): void {
  session.append('assistant/attempt', {
    turn,
    step,
    stream: [{ type: 'chunk', time: 0, chunk: { type: 'usage', usage } }],
  })
}

function cell(
  found: readonly RouteUsageEntry[],
  provider: string,
  model: string,
  day: string,
): RouteUsageEntry {
  const entry = found.find(item => item.provider === provider && item.model === model && item.day === day)
  if (entry === undefined) throw new Error(`no routeUsage entry for ${provider}/${model} on ${day}`)
  return entry
}

describe('dayKeyOf', () => {
  it('places a time on its UTC calendar day and refuses an unusable one', () => {
    expect(dayKeyOf(Date.UTC(2026, 2, 5, 0, 0, 0))).toBe(FIRST_DAY_KEY)
    expect(dayKeyOf(Date.UTC(2026, 2, 5, 23, 59, 59))).toBe(FIRST_DAY_KEY)
    expect(dayKeyOf(Date.UTC(2026, 2, 6, 0, 0, 0))).toBe('2026-03-06')
    expect(dayKeyOf(-1)).toBeNull()
    expect(dayKeyOf(Number.NaN)).toBeNull()
    expect(dayKeyOf(Number.MAX_SAFE_INTEGER + 1)).toBeNull()
  })
})

describe('routeUsage projection', () => {
  it('is an empty series before the first request', async () => {
    const { ctx, session } = await harness()
    expect(entries(ctx, session)).toEqual([])
    await ctx.fiber.dispose()
  })

  it('sums the disjoint buckets of several turns on one route', async () => {
    const { ctx, session } = await harness()
    settled(session, 1, 1, { inputTokens: 100, outputTokens: 20, cacheReadTokens: 400, cacheWriteTokens: 5, reasoningTokens: 8, totalTokens: 525 })
    settled(session, 2, 1, { inputTokens: 50, outputTokens: 10, cacheReadTokens: 200, cacheWriteTokens: 3, reasoningTokens: 4, totalTokens: 263 })
    const entry = cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY)
    expect(entry).toEqual({
      day: FIRST_DAY_KEY,
      provider: 'alpha',
      model: 'one',
      requests: 2,
      uncachedInputTokens: 150,
      outputTokens: 30,
      totalTokens: 788,
      cacheReadTokens: 600,
      cacheWriteTokens: 8,
      reasoningTokens: 12,
    })
    // Billed input is the three prompt buckets; the total adds output once, never reasoning twice.
    expect(entry.uncachedInputTokens + entry.cacheReadTokens! + entry.cacheWriteTokens! + entry.outputTokens)
      .toBe(entry.totalTokens)
    await ctx.fiber.dispose()
  })

  it('keeps separate cells for each provider and model route on the same day', async () => {
    const { ctx, session } = await harness()
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 11 })
    settled(session, 2, 1, { inputTokens: 20, outputTokens: 2, totalTokens: 22 }, { provider: 'alpha', model: 'two' })
    settled(session, 3, 1, { inputTokens: 30, outputTokens: 3, totalTokens: 33 }, { provider: 'beta', model: 'one' })
    expect(entries(ctx, session).map(entry => `${entry.provider}/${entry.model}:${entry.uncachedInputTokens}`))
      .toEqual(['alpha/one:10', 'alpha/two:20', 'beta/one:30'])
    await ctx.fiber.dispose()
  })

  it('spreads one session over several UTC days', async () => {
    const { ctx, session } = await harness(4)
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 11 })
    clock = FIRST_DAY + DAY
    settled(session, 2, 1, { inputTokens: 20, outputTokens: 2, totalTokens: 22 })
    clock = FIRST_DAY + 2 * DAY
    settled(session, 3, 1, { inputTokens: 30, outputTokens: 3, totalTokens: 33 })
    expect(entries(ctx, session).map(entry => `${entry.day}:${entry.uncachedInputTokens}`))
      .toEqual(['2026-03-05:10', '2026-03-06:20', '2026-03-07:30'])
    await ctx.fiber.dispose()
  })

  it('replaces the same slot instead of double counting an attempt and its message', async () => {
    const { ctx, session } = await harness()
    header(session, 'alpha', 'one')
    attempt(session, 1, 1, { inputTokens: 100, outputTokens: 5, cacheReadTokens: 10, cacheWriteTokens: 1, totalTokens: 116 })
    expect(cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY).requests).toBe(1)
    settled(session, 1, 1, { inputTokens: 100, outputTokens: 20, cacheReadTokens: 10, cacheWriteTokens: 1, totalTokens: 131 })
    expect(cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY)).toMatchObject({
      requests: 1,
      uncachedInputTokens: 100,
      outputTokens: 20,
      cacheReadTokens: 10,
      totalTokens: 131,
    })
    await ctx.fiber.dispose()
  })

  it('counts a retry as its own billed request', async () => {
    const { ctx, session } = await harness()
    header(session, 'alpha', 'one')
    attempt(session, 1, 1, { inputTokens: 40, outputTokens: 4, totalTokens: 44 })
    retryStarted(session, 1, 1, 2)
    attempt(session, 1, 2, { inputTokens: 60, outputTokens: 6, totalTokens: 66 })
    expect(cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY)).toMatchObject({
      requests: 2,
      uncachedInputTokens: 100,
      outputTokens: 10,
      totalTokens: 110,
    })
    await ctx.fiber.dispose()
  })

  it('omits a bucket only some requests reported', async () => {
    const { ctx, session } = await harness()
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, cacheReadTokens: 900, totalTokens: 911 })
    settled(session, 2, 1, { inputTokens: 20, outputTokens: 2, totalTokens: 22 })
    const merged = cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY)
    expect('cacheReadTokens' in merged).toBe(false)
    expect(merged.requests).toBe(2)
    expect(merged.uncachedInputTokens).toBe(30)
    await ctx.fiber.dispose()
  })

  it('discloses a bucket every counted request reported', async () => {
    const { ctx, session } = await harness()
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, cacheReadTokens: 5, totalTokens: 16 })
    const entry = cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY)
    expect(entry.cacheReadTokens).toBe(5)
    expect('cacheWriteTokens' in entry).toBe(false)
    expect('reasoningTokens' in entry).toBe(false)
    await ctx.fiber.dispose()
  })

  it('derives a request total from the full prompt split when the provider reports none', async () => {
    const { ctx, session } = await harness()
    settled(session, 1, 1, { inputTokens: 100, outputTokens: 20, cacheReadTokens: 7, cacheWriteTokens: 3 })
    expect(cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY).totalTokens).toBe(130)
    await ctx.fiber.dispose()
  })

  it('counts nothing for a usage record it cannot prove', async () => {
    const { ctx, session } = await harness()
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 11 })
    settled(session, 2, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 4 })
    settled(session, 3, 1, { inputTokens: 10, outputTokens: 1, reasoningTokens: 99 })
    settled(session, 4, 1, { inputTokens: 10, outputTokens: 1 })
    const entry = cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY)
    expect(entry.requests).toBe(1)
    expect(entry).toMatchObject({ uncachedInputTokens: 10, outputTokens: 1, totalTokens: 11 })
    await ctx.fiber.dispose()
  })

  it('attributes an attempt naming no route to the newest logged request header', async () => {
    const { ctx, session } = await harness()
    attempt(session, 1, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 11 })
    expect(entries(ctx, session)).toEqual([])
    header(session, 'gamma', 'vision')
    attempt(session, 2, 1, { inputTokens: 20, outputTokens: 2, totalTokens: 22 })
    expect(cell(entries(ctx, session), 'gamma', 'vision', FIRST_DAY_KEY).uncachedInputTokens).toBe(20)
    await ctx.fiber.dispose()
  })

  it('keeps the retention window and drops the day that falls out of it', async () => {
    const { ctx, session } = await harness(3)
    for (const offset of [0, 1, 2, 3]) {
      clock = FIRST_DAY + offset * DAY
      settled(session, offset + 1, 1, { inputTokens: offset + 1, outputTokens: 1, totalTokens: offset + 2 })
    }
    expect(entries(ctx, session).map(entry => entry.day)).toEqual(['2026-03-06', '2026-03-07', '2026-03-08'])
    await ctx.fiber.dispose()
  })

  it('does not retain a contribution older than the sliding window', async () => {
    const { ctx, session } = await harness(2)
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 11 })
    clock = FIRST_DAY + 5 * DAY
    settled(session, 2, 1, { inputTokens: 20, outputTokens: 2, totalTokens: 22 })
    expect(entries(ctx, session).map(entry => entry.day)).toEqual(['2026-03-10'])
    // An out-of-order stamp older than the window cannot rejoin it: the fold
    // retains days relative to the newest day it has seen, never wall-clock.
    clock = FIRST_DAY - DAY
    settled(session, 3, 1, { inputTokens: 30, outputTokens: 3, totalTokens: 33 })
    const after = entries(ctx, session)
    expect(after).toHaveLength(1)
    expect(cell(after, 'alpha', 'one', '2026-03-10').uncachedInputTokens).toBe(20)
    await ctx.fiber.dispose()
  })

  it('ignores a settlement whose event time no calendar day can hold', async () => {
    const { ctx, session } = await harness()
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 11 })
    clock = -1
    settled(session, 2, 1, { inputTokens: 99, outputTokens: 9, totalTokens: 108 })
    const after = entries(ctx, session)
    expect(after).toHaveLength(1)
    expect(cell(after, 'alpha', 'one', FIRST_DAY_KEY).uncachedInputTokens).toBe(10)
    await ctx.fiber.dispose()
  })

  it('ignores a header naming no route and reuses state for the route it already holds', async () => {
    const { ctx, session } = await harness()
    header(session, 'alpha', 'one')
    attempt(session, 1, 1, { inputTokens: 5, outputTokens: 1, totalTokens: 6 })
    const before = entries(ctx, session)
    header(session, '', '')
    header(session, 'alpha', 'one')
    expect(entries(ctx, session)).toStrictEqual(before)
    attempt(session, 2, 1, { inputTokens: 7, outputTokens: 1, totalTokens: 8 })
    expect(cell(entries(ctx, session), 'alpha', 'one', FIRST_DAY_KEY).requests).toBe(2)
    await ctx.fiber.dispose()
  })

  it('ignores a retry marker that opens no counted slot', async () => {
    const { ctx, session } = await harness()
    header(session, 'alpha', 'one')
    attempt(session, 1, 1, { inputTokens: 5, outputTokens: 1, totalTokens: 6 })
    const before = entries(ctx, session)
    retryStarted(session, 9, 9, 1)
    expect(entries(ctx, session)).toStrictEqual(before)
    await ctx.fiber.dispose()
  })

  it('ignores a settlement that reports no usage', async () => {
    const { ctx, session } = await harness()
    header(session, 'alpha', 'one')
    attempt(session, 1, 1, { inputTokens: 5, outputTokens: 1, totalTokens: 6 })
    const before = entries(ctx, session)
    session.append('assistant/attempt', { turn: 2, step: 1, stream: [] })
    expect(entries(ctx, session)).toStrictEqual(before)
    await ctx.fiber.dispose()
  })

  it('attributes a message naming no route to the newest logged request header', async () => {
    const { ctx, session } = await harness()
    header(session, 'delta', 'plain')
    settled(
      session,
      1,
      1,
      { inputTokens: 12, outputTokens: 3, totalTokens: 15 },
      { provider: '', model: '' },
    )
    expect(cell(entries(ctx, session), 'delta', 'plain', FIRST_DAY_KEY).uncachedInputTokens).toBe(12)
    await ctx.fiber.dispose()
  })

  it('empties one model cell while its provider sibling keeps counting', async () => {
    const { ctx, session } = await harness()
    header(session, 'alpha', 'one')
    attempt(session, 1, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 11 })
    settled(session, 2, 1, { inputTokens: 20, outputTokens: 2, totalTokens: 22 }, { provider: 'alpha', model: 'two' })
    // The (2, 1) replacement subtracts alpha/two first, so alpha/one is still in the cell map.
    settled(session, 2, 1, { inputTokens: 30, outputTokens: 3, totalTokens: 33 }, { provider: 'alpha', model: 'two' })
    const found = entries(ctx, session)
    expect(cell(found, 'alpha', 'one', FIRST_DAY_KEY).requests).toBe(1)
    expect(cell(found, 'alpha', 'two', FIRST_DAY_KEY)).toMatchObject({ requests: 1, uncachedInputTokens: 30 })
    await ctx.fiber.dispose()
  })

  it('admits a persisted checkpoint row and rejects one the fold no longer writes', async () => {
    const definition = createRouteUsageProjection(3)
    const { ctx, session } = await harness()
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, cacheReadTokens: 5, totalTokens: 16 })
    settled(session, 2, 1, { inputTokens: 20, outputTokens: 2, cacheReadTokens: 7, totalTokens: 29 })
    settled(session, 3, 1, { inputTokens: 5, outputTokens: 1, totalTokens: 6 }, { provider: 'beta', model: 'x' })
    const checkpoint = ctx.sessionProjections.checkpoint(session).routeUsage
    if (checkpoint === undefined) throw new Error('routeUsage checkpoint row is missing')
    const stored = checkpoint.val
    const row = definition.stateSchema.parse(stored)
    // Parsing the stored row returns it unchanged, so the checkpoint carries no
    // defaulted-away or re-added field. `toStrictEqual` is also the disclosure
    // test: the second route's cell omits `cacheReadTokens` because no request
    // there reported the bucket.
    expect(stored).toStrictEqual(row)
    expect(row.entries).toStrictEqual([
      {
        day: FIRST_DAY_KEY,
        provider: 'alpha',
        model: 'one',
        requests: 2,
        uncachedInputTokens: 30,
        outputTokens: 3,
        totalTokens: 45,
        cacheReadTokens: 12,
      },
      {
        day: FIRST_DAY_KEY,
        provider: 'beta',
        model: 'x',
        requests: 1,
        uncachedInputTokens: 5,
        outputTokens: 1,
        totalTokens: 6,
      },
    ])
    expect(() => definition.stateSchema.parse({
      ...structuredClone(row),
      days: { [FIRST_DAY_KEY]: { alpha: { one: { requests: -1 } } } },
    })).toThrow()
    // The in-flight slot keeps the same disclosure rule as the cells, so both
    // bucket shapes survive the checkpoint round trip.
    const slot = (extra: Record<string, number>): Record<string, unknown> => ({
      turn: 4,
      step: 1,
      contribution: {
        day: FIRST_DAY_KEY,
        provider: 'alpha',
        model: 'one',
        uncachedInputTokens: 100,
        outputTokens: 20,
        totalTokens: 120,
        ...extra,
      },
    })
    const reported = { ...structuredClone(row), last: slot({ cacheReadTokens: 60, cacheWriteTokens: 5, reasoningTokens: 8 }) }
    expect(definition.stateSchema.parse(reported)).toStrictEqual(reported)
    const silent = { ...structuredClone(row), last: slot({}) }
    expect(definition.stateSchema.parse(silent)).toStrictEqual(silent)
    await ctx.fiber.dispose()
  })

  it('removes the key when the mounting fiber disposes', async () => {
    const ctx = new Context()
    await ctx.plugin(SessionStore)
    await ctx.plugin(SessionProjectionRegistry)
    const fiber = await ctx.plugin(TokenMeter)
    const session = ctx.sessions.create()
    settled(session, 1, 1, { inputTokens: 10, outputTokens: 1, totalTokens: 11 })
    expect(entries(ctx, session)).toHaveLength(1)
    await fiber.dispose()
    expect(ctx.sessionProjections.snapshot(session).values.routeUsage).toBeUndefined()
    await ctx.fiber.dispose()
  })
})
