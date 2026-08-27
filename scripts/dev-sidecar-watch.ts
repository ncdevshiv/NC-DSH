/**
 * Rust sidecar watch for dev:desktop --hmr. Polls the ai-sidecar binary mtime
 * so a `cargo build` in the out-of-tree F:\alisia\ai-sdk checkout hot-swaps
 * without a manual host restart. The host's RuntimeSupervisor owns the
 * quiesce -> shadow -> healthProbe -> promote sequence; this stage only
 * signals that a rebuild happened. In-flight streams drain to a deadline
 * instead of being killed (see llm-ai-sdk/sidecar.ts drain()).
 *
 * Usage: node --import tsx/esm scripts/dev-sidecar-watch.ts [--binary <path>] [--poll[=ms]]
 * Requires an existing ai-sidecar binary; missing binary is polled until it appears.
 */

import { statSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const DEFAULT_BINARY = 'F:\\alisia\\ai-sdk\\target\\debug\\ai-sidecar.exe'
const DEFAULT_POLL_MS = 500

const args = process.argv.slice(2)
const binaryArg = args.find(a => a === '--binary' || a.startsWith('--binary=')) ?? args.find((_, i) => args[i - 1] === '--binary')
let binaryPath = DEFAULT_BINARY
if (binaryArg !== undefined) {
  if (binaryArg.startsWith('--binary=')) binaryPath = binaryArg.slice('--binary='.length)
  else if (binaryArg === '--binary') binaryPath = args[args.indexOf(binaryArg) + 1] ?? DEFAULT_BINARY
}
const pollArg = args.find(a => a === '--poll' || a.startsWith('--poll='))
let pollMs = DEFAULT_POLL_MS
if (pollArg !== undefined) {
  const raw = pollArg.startsWith('--poll=') ? pollArg.slice('--poll='.length) : '500'
  const parsed = Number(raw)
  if (!Number.isInteger(parsed) || parsed <= 0) {
    console.error(`dev-sidecar-watch: invalid --poll interval "${pollArg}"`)
    process.exit(1)
  }
  pollMs = parsed
}

const invokedPath = process.argv[1]
const isMain = invokedPath !== undefined && import.meta.url === pathToFileURL(resolve(invokedPath)).href
if (isMain) {
  let lastMtime = 0
  let lastSize = 0
  let seen = false
  const probe = (): { mtimeMs: number; size: number } | undefined => {
    try {
      const stat = statSync(binaryPath)
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
    console.log(`dev-sidecar-watch: watching ${binaryPath} (polling ${String(pollMs)}ms) — initial mtime ${String(lastMtime)} size ${String(lastSize)}`)
  } else {
    console.log(`dev-sidecar-watch: watching ${binaryPath} (polling ${String(pollMs)}ms) — binary not present yet, polling until it appears`)
  }

  const poll = (): void => {
    const current = probe()
    if (current === undefined) {
      if (seen) {
        console.log(`dev-sidecar-watch: binary gone: ${binaryPath}`)
        seen = false
      }
      return
    }
    if (!seen || current.mtimeMs !== lastMtime || current.size !== lastSize) {
      if (seen) {
        console.log(`dev-sidecar-watch: binary rebuilt: ${binaryPath} (mtime ${String(lastMtime)} -> ${String(current.mtimeMs)}, size ${String(lastSize)} -> ${String(current.size)}) — supervisor will quiesce, shadow, and promote`)
      }
      lastMtime = current.mtimeMs
      lastSize = current.size
      seen = true
    }
  }

  const handle = setInterval(poll, pollMs)
  if (typeof handle.unref === 'function') handle.unref()

  const onSignal = (): void => {
    process.exit(0)
  }
  process.once('SIGINT', onSignal)
  process.once('SIGTERM', onSignal)
}
