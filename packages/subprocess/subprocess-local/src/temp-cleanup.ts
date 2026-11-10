/**
 * Startup reclaim of the temporary trees this provider leaves behind.
 *
 * Every managed spawn uses a private per-process spill directory, and the Linux
 * scope additionally writes one private launch directory holding the complete
 * target environment. Both are removed on a JavaScript-observable host exit, so
 * what a `SIGKILL` or a power loss leaves behind is never reclaimed by the
 * process that made it: this module is that reclaim, owned by the provider that
 * creates the trees.
 *
 * Two rules keep it from touching live or foreign data. A tree is a candidate
 * only when its name has the exact `mkdtemp` shape this provider produces, and
 * only its oldest entry age is compared against the grace period — so a
 * concurrent provider instance keeps its own trees regardless of what it holds.
 * Contents decide the rest: a spill directory is removed only when empty (a
 * completed spill file is a retained recovery artifact, per the provider's
 * documented retention) and never when it is the running process's own, and a
 * launch directory only while it still holds the launch request the bootstrap
 * unlinks as it starts — the file whose survival proves the launch never
 * happened, and whose environment content is the reason to reclaim it. On POSIX
 * a directory owned by another account or writable by others is skipped outright,
 * because a name in a shared temporary directory proves nothing about who made it.
 * @module @deepseek-ai/dsh-subprocess-local/src/temp-cleanup
 */

import { lstat, readdir, rmdir, stat, unlink } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

/** Grace period for a provider temp tree: no healthy spawn or launch keeps one this long. */
const STALE_TEMP_GRACE_MS = 15 * 60_000

/** A per-process spill directory: `dsh-subprocess-` plus the six characters `mkdtemp` appends. */
const SPILL_DIR_RE = /^dsh-subprocess-[A-Za-z0-9]{6}$/

/** A per-launch directory: the same prefix with `launch-` before the `mkdtemp` suffix. */
const LAUNCH_DIR_RE = /^dsh-subprocess-launch-[A-Za-z0-9]{6}$/

/** The only file names the Linux launch protocol writes inside its directory. */
const LAUNCH_FILES = new Set(['launch-request.json', 'startup-error.json'])

/** The launch request the bootstrap unlinks as it starts; its survival marks an abandoned launch. */
const LAUNCH_REQUEST = 'launch-request.json'

/** A one-argument sink for contained failures; the sweep itself never rejects. */
export type TempWarnFn = (message: string) => void

/** Inputs to {@link sweepStaleProcessTemps}. */
export interface TempSweepOptions {
  /** Where a contained filesystem or trust failure is reported. */
  warn: TempWarnFn
  /** Directory scanned for this provider's temp trees; defaults to the OS temporary directory. */
  base?: string
  /** Grace period a tree must pass before it becomes a candidate. */
  graceMs?: number
  /**
   * The calling process's own spill directory, resolved when each candidate is
   * judged: an idle process's empty directory is still the one its next spill
   * opens into, and removing it would cost that run its full-output recovery.
   */
  keep?: () => string | undefined
}

/** The trees one sweep retired, by family. */
export interface TempSweepReport {
  /** Empty spill directories removed. */
  spillDirs: string[]
  /** Launch directories holding only launch-protocol files that were removed. */
  launchDirs: string[]
}

/** Whether another local OS user can replace children of this directory. */
async function isTrustedDirectory(path: string): Promise<boolean> {
  const stats = await lstat(path)
  if (!stats.isDirectory()) return false
  /* v8 ignore next -- Windows file permissions have no POSIX owner and mode equivalent. */
  if (process.platform === 'win32' || process.geteuid === undefined) return true
  /* v8 ignore start -- POSIX tests exercise the ownership and group-writable
     rejections that Windows metadata cannot represent. */
  return stats.uid === process.geteuid() && (stats.mode & 0o022) === 0
  /* v8 ignore stop */
}

/**
 * Last time a tree was worked on: its own modification time, which a child
 * create or remove refreshes, or its newest child when it still holds entries.
 * @param path - candidate directory, already confirmed a real directory.
 * @param names - its immediate entry names.
 * @returns the newest modification time in milliseconds.
 */
async function lastTouched(path: string, names: readonly string[]): Promise<number> {
  const directory = await stat(path)
  let newest = directory.mtimeMs
  for (const name of names) {
    const stats = await stat(join(path, name))
    if (stats.mtimeMs > newest) newest = stats.mtimeMs
  }
  return newest
}

/**
 * Reclaim this provider's stale temp trees under `base`. Failures are reported
 * through `warn` and contained, so a sweep can neither reject nor remove a tree
 * it cannot fully account for.
 * @param options - failure sink, scanned directory, and grace period.
 * @returns the paths removed, by family.
 */
export async function sweepStaleProcessTemps(options: TempSweepOptions): Promise<TempSweepReport> {
  const { warn } = options
  const base = options.base ?? tmpdir()
  const graceMs = options.graceMs ?? STALE_TEMP_GRACE_MS
  const report: TempSweepReport = { spillDirs: [], launchDirs: [] }
  let names: string[]
  try {
    names = await readdir(base)
  } catch (error: unknown) {
    warn(`subprocess-local: failed to scan ${base} for stale temp trees: ${String(error)}`)
    return report
  }
  const cutoff = Date.now() - graceMs
  for (const name of names) {
    const spill = SPILL_DIR_RE.test(name)
    if (!spill && !LAUNCH_DIR_RE.test(name)) continue
    const path = join(base, name)
    try {
      if (options.keep?.() === path) continue
      // lstat, never stat: a symlink has this provider's shape but its removal
      // would reach into a tree the provider never created.
      if (!(await isTrustedDirectory(path))) {
        warn(`subprocess-local: skipped unusable temp directory ${path}`)
        continue
      }
      const contents = await readdir(path)
      // Contents decide first, because they are what the sweep may never take.
      // A spill directory's full-output files are retained recovery artifacts,
      // and a launch directory qualifies only while its request file survives —
      // the bootstrap unlinks that request as it starts, so its presence after
      // the grace period means the launch never happened, and its absence means
      // this tree belongs to work that is settled or still running.
      if (spill) {
        if (contents.length > 0) continue
      } else if (!contents.includes(LAUNCH_REQUEST) || !contents.every(entry => LAUNCH_FILES.has(entry))) {
        continue
      }
      if (await lastTouched(path, contents) > cutoff) continue
      for (const entry of contents) await unlink(join(path, entry))
      await rmdir(path)
      if (spill) report.spillDirs.push(path)
      else report.launchDirs.push(path)
    } catch (error: unknown) {
      // The tree raced away, or another account took it over between the check
      // and the removal; both leave the next sweep to retry.
      warn(`subprocess-local: failed to retire temp tree ${path}: ${String(error)}`)
    }
  }
  return report
}
