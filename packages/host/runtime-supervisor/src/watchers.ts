/**
 * Runtime-supervisor watchers: file+binary polling that feeds the supervisor.
 * Each watcher is `spawnStage`-compatible (exponential backoff 1s·2ⁿ ≤30s,
 * 5 crashes → loud, 5min stable reset reused from scripts/dev-web.ts).
 * @module @deepseek-ai/dsh-runtime-supervisor/watchers
 */
import { statSync } from 'node:fs'

export interface BinaryWatcherOptions {
  readonly binaryPath: string
  readonly pollMs?: number
  readonly onRebuilt: (info: { mtimeMs: number; size: number; previous: { mtimeMs: number; size: number } | undefined }) => void
  readonly onGone?: (path: string) => void
}

export interface BinaryWatcherHandle {
  readonly close: () => void
}

/**
 * Poll one binary's mtime+size and invoke `onRebuilt` when the file appears
 * or its `mtimeMs`/`size` changes. Matches `scripts/dev-sidecar-watch.ts`
 * semantics (polling, not fs.watch) so network mounts and out-of-tree crates
 * both fire.
 * @param options - binary path, poll interval, and callbacks.
 * @returns handle to stop polling.
 */
export function watchBinary(options: BinaryWatcherOptions): BinaryWatcherHandle {
  const pollMs = options.pollMs ?? 500
  let lastMtime = 0
  let lastSize = 0
  let seen = false

  const probe = (): { mtimeMs: number; size: number } | undefined => {
    try {
      const stat = statSync(options.binaryPath)
      return { mtimeMs: stat.mtimeMs, size: stat.size }
    } catch {
      return undefined
    }
  }

  const initial = probe()
  if (initial !== undefined) {
    lastMtime = initial.mtimeMs
    lastSize = initial.size
    seen = true
  }

  const poll = (): void => {
    const current = probe()
    if (current === undefined) {
      if (seen) {
        seen = false
        options.onGone?.(options.binaryPath)
      }
      return
    }
    if (!seen || current.mtimeMs !== lastMtime || current.size !== lastSize) {
      const previous = seen ? { mtimeMs: lastMtime, size: lastSize } : undefined
      lastMtime = current.mtimeMs
      lastSize = current.size
      seen = true
      options.onRebuilt({ mtimeMs: current.mtimeMs, size: current.size, previous })
    }
  }

  const handle = setInterval(poll, pollMs)
  if (typeof handle.unref === 'function') handle.unref()

  return {
    close: () => { clearInterval(handle) },
  }
}

/**
 * Totar for lock-file generations state persisted to `dsh-runtime-supervisor.lock`.
 * @param store - helper that reads/writes the JSON file.
 */
export interface SupervisorLockStore {
  readonly path: string
  readonly read: () => {
    generations: readonly { id: string; pid?: number; buildRev: string; phase: string; kind: string }[]
    buildRev?: string
  } | undefined
  readonly write: (value: {
    generations: readonly { id: string; pid?: number; buildRev: string; phase: string; kind: string }[]
    buildRev: string
  }) => void
}
