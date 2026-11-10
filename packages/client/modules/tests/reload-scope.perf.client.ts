// @vitest-environment jsdom
/**
 * Opt-in synthetic benchmark for the live client-edit path: how much a rebuilt
 * bundle actually refreshes, how long the swap takes, and what a failed swap
 * costs to undo. Manual lane only (`bun run test:web:perf`); the normal test
 * inventories do not match this file name.
 */

import { performance } from 'node:perf_hooks'
import { afterAll, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import Loader from '@deepseek-ai/cordis-plugin-loader'
import { apply as provideModules, createClientModuleSystem } from '../src/client/index.ts'
import type {
  ClientBundleRegistration, ClientModuleLoaderTarget, WebBootEntry, WebBootGraph,
} from '../src/client/index.ts'

const contexts: Context[] = []
afterAll(async () => {
  for (const ctx of contexts.splice(0)) {
    await ctx.fiber.dispose()
    await ctx.fiber.await()
  }
})

/** Graph row with the revision-stamped URL the Host serves for one package. */
const row = (id: string, rev = 'r0', extra: Partial<WebBootEntry> = {}): WebBootEntry => ({
  id, rev, url: `/plugins/??${id}/client.js&rev=${rev}`, ...extra,
})
const graph = (...entries: WebBootEntry[]): WebBootGraph => ({
  rev: JSON.stringify(entries), entries,
  batches: entries.length === 0 ? [] : [{
    phase: 'application', url: '/batch', rev: 'batch', entries: entries.map(each => each.id),
  }],
})

const SHARED = 'shared'
/** Consumers that import the shared bundle, so a shared edit has to reach them. */
const importers = (count: number, rev = 'r0'): WebBootEntry[] =>
  Array.from({ length: count }, (_, index) =>
    row(`consumer-${String(index)}`, rev, { external: [`${SHARED}/client`] }))

interface Counters {
  /** Bundle URLs this page was asked to deliver. */
  delivered: number
  /** Module-table materializations, including package-local chunks. */
  materialized: number
  /** Plugin activations and their teardowns. */
  mounted: number
  disposed: number
}

interface Bench {
  ctx: Context
  modules: ReturnType<typeof createClientModuleSystem>
  counters: Counters
  /** Replace the shared bundle's registered code. */
  setShared: (factory: ClientBundleRegistration['factory']) => void
  setConsumer: (factory: ClientBundleRegistration['factory']) => void
}

/** A page whose bundles arrive through a counting transport, over a real Loader. */
async function page(rows: readonly WebBootEntry[], revision: 'r0' | 'broken'): Promise<Bench> {
  const ctx = new Context()
  contexts.push(ctx)
  const counters: Counters = { delivered: 0, materialized: 0, mounted: 0, disposed: 0 }
  const factories: Record<string, ClientBundleRegistration['factory']> = {
    [SHARED]: () => {
      counters.materialized++
      return {
        apply(ctx: Context) {
          ctx.effect(() => {
            counters.mounted++
            return () => { counters.disposed++ }
          })
        },
        revision: 'r0',
      }
    },
  }
  for (const entry of rows) {
    if (entry.id === SHARED) continue
    factories[entry.id] = (require) => {
      counters.materialized++
      const shared = require(`${SHARED}/client`) as { revision: string }
      return {
        apply(ctx: Context) {
          ctx.effect(() => {
            counters.mounted++
            return () => { counters.disposed++ }
          })
        },
        shared,
      }
    }
  }
  const target: ClientModuleLoaderTarget = {
    mode: 'queue', pendingQueue: [], load: () => {},
    create: options => createClientModuleSystem(target, {
      id: 'bootstrap', exports: { inject: ['loader'], apply: provideModules },
    }, options),
  }
  const modules = target.create({
    boot: graph(...rows), staticModules: {},
    loadBundle: async (url) => {
      const ids = url === '/batch'
        ? rows.map(each => each.id)
        : [url.split('??')[1]!.split('/client.js')[0]!]
      for (const id of ids) {
        if (id === SHARED && revision === 'broken') {
          counters.delivered++
          target.load({
            id,
            factory: () => ({ apply() { throw new Error('rebuilt apply unavailable') } }),
          })
          continue
        }
        const factory = factories[id]
        if (factory === undefined) continue
        counters.delivered++
        target.load({ id, factory })
      }
    },
  })
  await ctx.plugin(Loader)
  ctx.loader.internal = modules as never
  await modules.entries.start(ctx.loader, modules.manifest)
  return {
    ctx,
    modules,
    counters,
    setShared: (factory) => { factories[SHARED] = factory },
    setConsumer: (factory) => {
      for (const entry of rows) if (entry.id !== SHARED) factories[entry.id] = factory
    },
  }
}

/** A rebuilt shared bundle, staged at a fresh revision. */
function rebuiltShared(counters: Counters): ClientBundleRegistration['factory'] {
  return () => {
    counters.materialized++
    return {
      apply(ctx: Context) {
        ctx.effect(() => {
          counters.mounted++
          return () => { counters.disposed++ }
        })
      },
      revision: 'r1',
    }
  }
}

function median(values: readonly number[]): number {
  const sorted = [...values].sort((left, right) => left - right)
  const middle = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 1
    ? sorted[middle]!
    : (sorted[middle - 1]! + sorted[middle]!) / 2
}
const rounded = (value: number): number => Math.round(value * 100) / 100

interface Swap {
  readonly medianMs: number
  readonly samplesMs: readonly number[]
  readonly delivered: number
  readonly remounted: number
}

/**
 * One shared-bundle edit measured end to end: the swap's wall clock, the bundles
 * it delivered, and the entries it remounted. `attempts` repeats the edit against
 * a fresh page so the samples stay independent.
 */
async function editShared(rows: readonly WebBootEntry[], attempts: number): Promise<Swap> {
  const samplesMs: number[] = []
  const scopes: { delivered: number; remounted: number }[] = []
  for (let attempt = 0; attempt < attempts; attempt++) {
    const bench = await page(rows, 'r0')
    const consumerCount = rows.filter(each => each.id !== SHARED).length
    expect(await bench.modules.import(SHARED)).toMatchObject({ revision: 'r0' })
    bench.setShared(rebuiltShared(bench.counters))
    const before = { ...bench.counters }
    const start = performance.now()
    await bench.modules.entries.reload(SHARED, 'r1')
    samplesMs.push(performance.now() - start)
    scopes.push({ delivered: bench.counters.delivered - before.delivered, remounted: bench.counters.mounted - before.mounted })
    expect(await bench.modules.import(SHARED)).toMatchObject({ revision: 'r1' })
    // Every importer reads the new exports, and nothing else on the page moves.
    for (const entry of rows) {
      if (entry.id === SHARED) continue
      expect(await bench.modules.import(entry.id)).toMatchObject({ shared: { revision: 'r1' } })
    }
    expect(scopes.at(-1)?.remounted).toBe(consumerCount + 1)
  }
  return {
    medianMs: rounded(median(samplesMs)),
    samplesMs: samplesMs.map(rounded),
    delivered: scopes.at(-1)!.delivered,
    remounted: scopes.at(-1)!.remounted,
  }
}

it('measures what one shared client-bundle edit refreshes and costs', async () => {
  const sizes = [8, 32, 64]
  const results: unknown[] = []
  for (const count of sizes) {
    const rows = [...importers(count), row(SHARED)]
    const edit = await editShared(rows, 5)
    results.push({
      consumers: count,
      // A page reload delivers every row; the live edit delivers only the edit.
      reloadEquivalentDeliveries: rows.length,
      editDeliveries: edit.delivered,
      entriesRemounted: edit.remounted,
      medianMs: edit.medianMs,
      samplesMs: edit.samplesMs,
    })
  }
  process.stdout.write(`CLIENT_SHARED_EDIT_PERF_RESULT ${JSON.stringify(results)}\n`)
}, 180_000)

it('measures a page-wide graph sync against the single-edit path', async () => {
  const rows = [...importers(32), row(SHARED)]
  const samplesMs: number[] = []
  const scopes: number[] = []
  for (let attempt = 0; attempt < 5; attempt++) {
    const bench = await page(rows, 'r0')
    bench.setShared(rebuiltShared(bench.counters))
    bench.setConsumer((require) => {
      bench.counters.materialized++
      const shared = require(`${SHARED}/client`) as { revision: string }
      return {
        apply(ctx: Context) {
          ctx.effect(() => {
            bench.counters.mounted++
            return () => { bench.counters.disposed++ }
          })
        },
        shared,
        revision: 'r1',
      }
    })
    const before = bench.counters.mounted
    const start = performance.now()
    await bench.modules.entries.sync(graph(...rows.map(each => row(each.id, 'r1',
      each.external === undefined ? {} : { external: each.external }))))
    samplesMs.push(performance.now() - start)
    scopes.push(bench.counters.mounted - before)
    expect(bench.modules.entries.state.getSnapshot()).toEqual({ syncing: false, failures: [] })
    // A page-wide sync is the whole page: every row is delivered and remounted.
    expect(scopes.at(-1)).toBe(rows.length)
  }
  const edit = await editShared(rows, 1)
  const report = {
    rows: rows.length,
    syncMedianMs: rounded(median(samplesMs)),
    syncEntriesRemounted: scopes.at(-1),
    syncDeliveries: rows.length,
    editMedianMs: edit.medianMs,
    editDeliveries: edit.delivered,
    editEntriesRemounted: edit.remounted,
    samplesMs: samplesMs.map(rounded),
  }
  process.stdout.write(`CLIENT_GRAPH_SYNC_VS_EDIT_PERF_RESULT ${JSON.stringify(report)}\n`)
}, 180_000)

it('measures what a failed live edit costs to revert', async () => {
  const rows = [...importers(32), row(SHARED)]
  const samplesMs: number[] = []
  for (let attempt = 0; attempt < 5; attempt++) {
    const bench = await page(rows, 'r0')
    const sharedBefore = await bench.modules.import(SHARED)
    const mountedBefore = bench.counters.mounted
    bench.setShared(() => { throw new Error('rebuilt bundle will not materialize') })
    const start = performance.now()
    await expect(bench.modules.entries.reload(SHARED, 'r1')).rejects.toThrow('rebuilt bundle will not materialize')
    samplesMs.push(performance.now() - start)
    // The page is left on the pair it was running, not on a half-swapped one.
    expect(await bench.modules.import(SHARED)).toBe(sharedBefore)
    // Restoring costs one more activation: the rollback re-mounts the previous bundle.
    expect(bench.counters.mounted).toBe(mountedBefore + 1)
    for (const entry of rows) {
      if (entry.id === SHARED) continue
      expect(await bench.modules.import(entry.id)).toMatchObject({ shared: { revision: 'r0' } })
    }
  }
  const rejectedDelivery = rounded(median(samplesMs))
  const activateFailure: number[] = []
  {
    const bench = await page(rows, 'broken')
    const mountedBefore = bench.counters.mounted
    const start = performance.now()
    await expect(bench.modules.entries.reload(SHARED, 'r1')).rejects.toThrow('rebuilt apply unavailable')
    activateFailure.push(performance.now() - start)
    expect(bench.counters.disposed).toBeLessThanOrEqual(mountedBefore)
  }
  process.stdout.write(`CLIENT_EDIT_ROLLBACK_PERF_RESULT ${JSON.stringify({
    rows: rows.length,
    rejectedMaterializationMs: rejectedDelivery,
    activateFailureMs: rounded(median(activateFailure)),
    samplesMs: samplesMs.map(rounded),
  })}\n`)
}, 180_000)
