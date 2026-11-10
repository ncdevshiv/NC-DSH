/**
 * Atomic whole-file replacement for the JSON backend.
 *
 * Publish protocol: write a same-directory temp file, fsync it, then
 * `rename()` over the target. Rename is an atomic replace on POSIX and on
 * Windows (libuv maps it to `MoveFileExW(..., MOVEFILE_REPLACE_EXISTING)`),
 * and replacement is the intended semantic here — unlike the session-log
 * backend's link()+unlink() no-clobber protocol, a unit file has exactly one
 * writer per process and last-write-wins is correct. After the rename the
 * parent directory is fsynced on POSIX so the new entry is crash-durable.
 * @module @deepseek-ai/dsh-storage-json/src/atomic
 */

import { open, readdir, rename, rm, stat } from 'node:fs/promises'
import type { Dirent } from 'node:fs'
import { dirname, join } from 'node:path'
import { randomUUID } from 'node:crypto'

/**
 * Age a publish scratch file must pass before a sweep retires it. A live publish owns its
 * scratch file until the rename, so the guard is the only thing separating residue from an
 * in-flight write; it is generous because the sweep exists solely to reclaim what an
 * unclean exit left behind, which no process can pin.
 */
const STALE_PUBLISH_TEMP_MS = 15 * 60_000

/** A publish scratch file name: dot-prefixed random-UUID text beside its target. */
const PUBLISH_TEMP = /^\.[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}\.tmp$/

/**
 * Retire one scratch file when it is older than `cutoff`.
 * @param path - scratch file the listing found.
 * @param cutoff - modification times after this instant belong to a live publish.
 * @returns whether the file was removed.
 */
async function retireIfStale(path: string, cutoff: number): Promise<boolean> {
  let stamped: number | undefined
  try {
    stamped = (await stat(path)).mtimeMs
  } catch (_error) {
    // Gone between the listing and the stat, or unreadable: nothing to retire.
    return false
  }
  if (stamped > cutoff) return false
  try {
    await rm(path, { force: true })
  } catch (_error) {
    // Pinned by a live publish in another process; the next open retries.
    return false
  }
  return true
}

/** Scratch files retired per batch: enough to overlap the filesystem round trips, few enough to stay away from descriptor limits. */
const RETIRE_CONCURRENCY = 32

/**
 * Remove publish scratch files that an unclean exit left in the unit tree under
 * `root`, walking every subdirectory the layouts publish into. Only files past
 * the sweep age are retired, so a concurrent publish in another process keeps
 * its own scratch file. This is total: a directory that cannot be listed holds
 * no reclaimable entry the caller could reach, so the tree walk stops there
 * rather than failing the unit open that triggered it. Retirals run in batches
 * so a heavily polluted tree costs round trips, not one serial call per file.
 * @param root - backend root directory; an absent root has no residue.
 * @param olderThanMs - scratch age to retire.
 * @returns the retired paths.
 */
export async function sweepStalePublishTemps(root: string, olderThanMs = STALE_PUBLISH_TEMP_MS): Promise<string[]> {
  const cutoff = Date.now() - olderThanMs
  const candidates: string[] = []
  const directories = [root]
  for (const directory of directories) {
    let entries: Dirent[]
    try {
      entries = await readdir(directory, { withFileTypes: true })
    } catch (_error) {
      // Absent or unlistable: no reclaimable entry here, and the unit open
      // that needs this path reports its own failure.
      continue
    }
    for (const entry of entries) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) {
        directories.push(path)
        continue
      }
      if (PUBLISH_TEMP.test(entry.name)) candidates.push(path)
    }
  }
  const removed: string[] = []
  for (let index = 0; index < candidates.length; index += RETIRE_CONCURRENCY) {
    const batch = candidates.slice(index, index + RETIRE_CONCURRENCY)
    const retired = await Promise.all(batch.map(path => retireIfStale(path, cutoff)))
    batch.forEach((path, offset) => {
      if (retired[offset] === true) removed.push(path)
    })
  }
  return removed
}

/**
 * Durably replace `path` with `data`.
 * @param path - Absolute target file path.
 * @param data - Full new file content.
 * @returns resolution after the replacement is crash-durable.
 */
export async function writeAtomic(path: string, data: string): Promise<void> {
  const tmp = join(dirname(path), `.${randomUUID()}.tmp`)
  try {
    const handle = await open(tmp, 'wx', 0o600)
    try {
      await handle.writeFile(data, 'utf8')
      await handle.sync()
    } finally {
      await handle.close()
    }
    await rename(tmp, path)
    await fsyncDirectory(dirname(path))
  } catch (error) {
    await rm(tmp, { force: true })
    throw error
  }
}

/** fsync a POSIX directory so a just-renamed entry is crash-durable. */
/* v8 ignore start -- Windows rejects O_RDONLY directory opens; POSIX coverage exercises this. */
async function fsyncDirectory(path: string): Promise<void> {
  if (process.platform === 'win32') return
  const handle = await open(path, 'r')
  try {
    await handle.sync()
  } finally {
    await handle.close()
  }
}
/* v8 ignore stop */
