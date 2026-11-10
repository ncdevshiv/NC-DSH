/** The diagnostics rings: bounded retention, latency percentiles, health, and crash records. */

import { Context } from '@deepseek-ai/cordis'
import { describe, expect, it, vi } from 'vitest'
import { Ring } from '../src/rings.ts'
import { TelemetryRuntime, type Config } from '../src/index.ts'

/** Mount the service over a stub Loader and no HTTP endpoint. */
function bench(overrides: Partial<Config> = {}) {
  const ctx = new Context()
  const entries = vi.fn(() => [] as unknown[])
  ctx.provide('loader', { entries } as never)
  const telemetry = new TelemetryRuntime(ctx, {
    events: 4,
    clientSpans: 2,
    crashes: 2,
    slowMs: 50,
    maxBodyBytes: 1024,
    serveDiagnostics: false,
    ...overrides,
  } as Required<Config>)
  return { ctx, telemetry, entries }
}

describe('Ring', () => {
  it('keeps the newest entries in order and never grows', () => {
    const ring = new Ring<number>(3)
    for (const value of [1, 2, 3, 4, 5]) ring.push(value)
    expect(ring.size).toBe(3)
    expect(ring.toArray()).toEqual([3, 4, 5])
  })

  it('returns a short ring oldest-first', () => {
    const ring = new Ring<number>(3)
    ring.push(1)
    ring.push(2)
    expect(ring.toArray()).toEqual([1, 2])
  })
})

describe('TelemetryRuntime', () => {
  it('aggregates span latency and counts', () => {
    const { telemetry } = bench()
    telemetry.record('tool', 'tool:read', 10, { ok: true })
    telemetry.record('tool', 'tool:read', 30, { ok: true })
    telemetry.record('tool', 'tool:read', 60, { ok: false })

    const snapshot = telemetry.snapshot()
    const summary = snapshot.latency.find(entry => entry.name === 'tool:read')!
    expect(summary).toMatchObject({ count: 3, errors: 1, max: 60, p50: 30 })
    expect(snapshot.counters['span.slow']).toBe(1)
    expect(snapshot.health.ok).toBe(true)
  })

  it('retains only the newest events and reports a recent crash as unhealthy', () => {
    const { telemetry } = bench()
    for (let index = 0; index < 6; index += 1) telemetry.record('tool', `tool:${String(index)}`, 5)
    expect(telemetry.snapshot().events.map(event => event.name)).toEqual(['tool:2', 'tool:3', 'tool:4', 'tool:5'])

    telemetry.crash('tool', new Error('boom\nsecond line'))
    const snapshot = telemetry.snapshot()
    expect(snapshot.crashes[0]).toMatchObject({ scope: 'tool', message: 'boom' })
    expect(snapshot.crashes[0]?.stack).toContain('second line')
    expect(snapshot.health).toMatchObject({ ok: false })
    expect(snapshot.health.reasons[0]).toContain('recent tool crash: boom')
  })

  it('accepts browser spans and bounds them', () => {
    const { telemetry } = bench()
    telemetry.client({ at: new Date().toISOString(), kind: 'rpc', name: 'session/list', durationMs: 12, ok: true })
    telemetry.client({ at: new Date().toISOString(), kind: 'rpc', name: 'session/list', durationMs: 20, ok: true })
    telemetry.client({ at: new Date().toISOString(), kind: 'rpc', name: 'session/list', durationMs: 30, ok: true })

    const snapshot = telemetry.snapshot()
    expect(snapshot.client.map(span => span.durationMs)).toEqual([20, 30])
    expect(snapshot.counters['client.rpc']).toBe(3)
  })

  it('notifies subscribers and contains a throwing one', () => {
    const { telemetry } = bench()
    const seen: string[] = []
    telemetry.subscribe(() => { throw new Error('subscriber failed') })
    telemetry.subscribe(event => seen.push(event.name))
    telemetry.record('model', 'p/m', 5)
    expect(seen).toEqual(['p/m'])
    expect(telemetry.snapshot().counters['telemetry.subscriber-errors']).toBe(1)
  })

  it('reports a failing loader row as an unhealthy reason', () => {
    const { telemetry, entries } = bench()
    entries.mockReturnValue([
      { options: { id: 'broken', name: '@x/broken' }, fiber: { state: 3, error: new Error('npm load failed') } },
      { options: { id: 'fine', name: '@x/fine' }, fiber: { state: 2 } },
    ] as never)
    const snapshot = telemetry.snapshot()
    expect(snapshot.loader).toEqual([
      { id: 'broken', name: '@x/broken', state: 'FAILED', error: 'npm load failed' },
      { id: 'fine', name: '@x/fine', state: 'ACTIVE' },
    ])
    expect(snapshot.health.ok).toBe(false)
  })
})
