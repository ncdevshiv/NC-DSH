/**
 * Threshold-free measurement of the startup clock-marker retire: the HMR service
 * calls it synchronously inside `[Service.init]`, so the question is how long a
 * developer's boot waits on it when the temporary root is crowded. Only entries
 * past the file-timestamp granularity are retired, and only the prefixed
 * directories are stat'd, so the cost is one listing plus a small number of
 * calls. Diagnostic only, not a gate; run with
 * `bunx tsx packages/boot/hmr/tests/clock-sweep.perf.ts`.
 */

import { mkdir, mkdtemp, rm, utimes, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'
import { sweepStaleClockDirectories } from '../src/module-files.ts'

/** The clock-marker prefix, mirroring `module-files.ts`; the sweep matches on it. */
const CLOCK_DIRECTORY_PREFIX = 'dsh-hmr-clock-'

/** Residue a killed watcher run leaves, per tree. */
const MARKER_COUNTS = [10, 100, 1_000] as const
/** Unrelated entries crowding the scanned root, as on a developer machine. */
const NOISE_ENTRIES = 5_000
/** Runs measured per size. */
const SAMPLES = 5
/** Markers older than the file-timestamp granularity are residue. */
const AGED = new Date(Date.now() - 60_000)

const results: string[] = []
for (const markers of MARKER_COUNTS) {
  const times: number[] = []
  let retired = 0
  for (let sample = 0; sample < SAMPLES; sample++) {
    const root = await mkdtemp(join(tmpdir(), 'dsh-clock-sweep-perf-'))
    for (let index = 0; index < NOISE_ENTRIES; index++) await writeFile(join(root, `noise-${index}.txt`), 'x', 'utf8')
    const paths: string[] = []
    for (let index = 0; index < markers; index++) {
      const path = join(root, `${CLOCK_DIRECTORY_PREFIX}${index}`)
      await mkdir(path)
      await writeFile(join(path, 'clock.txt'), '1', 'utf8')
      await utimes(path, AGED, AGED)
      paths.push(path)
    }
    // A live second watcher's marker shares the prefix and must survive.
    const live = join(root, `${CLOCK_DIRECTORY_PREFIX}live`)
    await mkdir(live)
    await writeFile(join(live, 'clock.txt'), '1', 'utf8')
    const started = performance.now()
    retired = (await sweepStaleClockDirectories(root)).length
    times.push(performance.now() - started)
    if (retired !== markers) throw new Error(`retired ${String(retired)} of ${String(markers)}`)
    if (!existsSync(live) || !existsSync(join(root, 'noise-0.txt'))) throw new Error('the retire reached outside its own residue')
    if (existsSync(paths[0] as string)) throw new Error('a retired marker survived')
    await rm(root, { recursive: true, force: true })
  }
  const sorted = [...times].sort((a, b) => a - b)
  results.push(
    `HMR_CLOCK_SWEEP_PERF_RESULT markers=${markers} noise=${NOISE_ENTRIES} `
    + `sweep_ms_median=${(sorted[Math.floor(sorted.length / 2)] ?? 0).toFixed(2)} retired=${retired}`,
  )
}
console.log(results.join('\n'))
