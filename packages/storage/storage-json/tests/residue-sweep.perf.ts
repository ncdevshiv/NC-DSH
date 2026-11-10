/**
 * Threshold-free measurements of the JSON backend's residue sweep: the boot cost
 * the auto-heal adds, and whether it reclaims what repeated unclean publishes
 * accumulate without touching unit data. Residue is written the way
 * `writeAtomic` leaves it — a same-directory UUID scratch file that never got
 * renamed. Diagnostic only, not a gate; run with
 * `bunx tsx packages/storage/storage-json/tests/residue-sweep.perf.ts`.
 */

import { mkdir, mkdtemp, readFile, rm, utimes, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { randomUUID } from 'node:crypto'
import { performance } from 'node:perf_hooks'
import { JsonStorageBackend } from '../src/index.ts'
import { sweepStalePublishTemps } from '../src/atomic.ts'

/** Scratch files an unclean exit left, per tree. */
const RESIDUE_COUNTS = [50, 500, 2_000] as const
/** Unit directories the residue is spread across, mirroring the per-record tree. */
const UNIT_COUNTS = [10, 50, 50] as const
/** Runs measured per size. */
const SAMPLES = 5
/** Every scratch file is written this far in the past so the sweep retires it. */
const AGED = new Date(Date.now() - 20 * 60_000)

function elapsed(fn: () => Promise<unknown>): Promise<number> {
  const start = performance.now()
  return fn().then(() => performance.now() - start)
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b)
  return sorted[Math.floor(sorted.length / 2)] ?? 0
}

/** One scratch file beside its target, exactly as a killed publish leaves it. */
async function writeResidue(dir: string): Promise<string> {
  const path = join(dir, `.${randomUUID()}.tmp`)
  await writeFile(path, '{"unit":{"name":"u","version":1}}', 'utf8')
  await utimes(path, AGED, AGED)
  return path
}

/** A root holding `units` per-record-shaped unit trees, each with records and stale scratch. */
async function buildRoot(residue: number, units: number): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), 'dsh-residue-perf-'))
  for (let unit = 0; unit < units; unit++) {
    const dir = join(root, `unit-${unit}`, 'records')
    await mkdir(dir, { recursive: true, mode: 0o700 })
    for (let record = 0; record < Math.ceil(residue / units); record++) {
      await writeFile(join(dir, `r-${record}.json`), '{"unit":{"name":"u","version":1},"value":1}', 'utf8')
      await writeResidue(dir)
    }
  }
  return root
}

const results: string[] = []

for (const [index, residue] of RESIDUE_COUNTS.entries()) {
  const units = UNIT_COUNTS[index] ?? 10
  const times: number[] = []
  let swept = 0
  let records = 0
  for (let sample = 0; sample < SAMPLES; sample++) {
    const root = await buildRoot(residue, units)
    records += Math.ceil(residue / units) * units
    const started = performance.now()
    const removed = await sweepStalePublishTemps(root)
    times.push(performance.now() - started)
    swept += removed.length
    // A sweep that ate the unit documents would show up as a survivor count here.
    const survivors = await sweepStalePublishTemps(root)
    if (survivors.length > 0) throw new Error('a retired scratch file was still listed')
    await rm(root, { recursive: true, force: true })
  }
  results.push(
    `STORAGE_RESIDUE_SWEEP_PERF_RESULT residue=${residue} units=${units} `
    + `sweep_ms_median=${median(times).toFixed(2)} reclaimed=${Math.round(swept / SAMPLES)} `
    + `record_documents=${Math.round(records / SAMPLES)}_per_run`,
  )
}

// Boot cost of the heal: the sweep runs once per backend at the first unit open,
// so a first open on a residue-bearing root is the path a real restart pays.
const descriptor = { name: 'metrics', version: 1, tables: ['records'], hasGlobal: false }
const cost = { clean: [] as number[], healed: [] as number[], later: [] as number[] }
for (let sample = 0; sample < SAMPLES; sample++) {
  const empty = await mkdtemp(join(tmpdir(), 'dsh-residue-open-'))
  const cleanBackend = new JsonStorageBackend(empty)
  cost.clean.push(await elapsed(() => cleanBackend.kv.open(descriptor).then(unit => unit.close())))
  await cleanBackend.close()
  await rm(empty, { recursive: true, force: true })

  const root = await buildRoot(500, 50)
  const backend = new JsonStorageBackend(root)
  cost.healed.push(await elapsed(() => backend.kv.open(descriptor).then(unit => unit.close())))
  cost.later.push(await elapsed(() => backend.kv.open({ ...descriptor, name: 'second' }).then(unit => unit.close())))
  // The reclaim is launched by the first open and settles by close(), so the
  // reclaim's completion is observed where the backend owns it.
  await backend.close()
  if (!existsSync(join(root, 'unit-0', 'records', 'r-0.json'))) throw new Error('the first open removed a record document')
  if ((await sweepStalePublishTemps(root)).length > 0) throw new Error('the backend left residue behind after close')
  await rm(root, { recursive: true, force: true })
}
results.push(
  `STORAGE_BOOT_HEAL_PERF_RESULT open_clean_ms_median=${median(cost.clean).toFixed(2)} `
  + `open_with_sweep_ms_median=${median(cost.healed).toFixed(2)} `
  + `later_open_ms_median=${median(cost.later).toFixed(2)}`,
)

// Repeated unclean publishes leave one scratch file each; the reopen must
// reclaim them and keep every record committed before the crash.
const root = await mkdtemp(join(tmpdir(), 'dsh-residue-durable-'))
const writer = new JsonStorageBackend(root)
const written = await writer.kv.open(descriptor)
await written.putRecord('records', 'kept', { value: 42 })
const scratch = await writeResidue(root)
await written.close()
await writer.close()
const next = new JsonStorageBackend(root)
const reopened = await next.kv.open(descriptor)
const stored = await readFile(join(root, 'metrics.json'), 'utf8')
if (!stored.includes('"kept"')) throw new Error('reopen lost a record committed before the crash')
await reopened.close()
await next.close()
if (existsSync(scratch)) throw new Error('the reopened backend did not reclaim the residue')
await rm(root, { recursive: true, force: true })
results.push('STORAGE_RESIDUE_SWEEP_PERF_RESULT crash_residue_reclaimed=1 committed_record_kept=1')

console.log(results.join('\n'))
