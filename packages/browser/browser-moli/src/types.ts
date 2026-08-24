/**
 * Injectable-boundary and option types for the moli browser provider. Types
 * only — no runtime code.
 * @module @deepseek-ai/dsh-browser-moli/types
 */

/** The process handle the session needs from the spawn boundary. */
export interface SpawnedProcess {
  readonly pid: number | undefined
  /** Terminate the child; platform-default signal semantics apply. */
  kill(): void
}

/** A spawned `moli serve` child process. */
export type SpawnFn = (binaryPath: string, args: readonly string[]) => SpawnedProcess

/** A minimal fetch boundary so readiness polling and target discovery are injectable. */
export type FetchFn = (url: string, init?: { method?: string; signal?: AbortSignal }) => Promise<Response>

/** A minimal WebSocket constructor boundary for the CDP connection. */
export type WebSocketFactory = (url: string) => WebSocket

/** Complete provider options after the plugin resolves every default. */
export interface MoliBrowserProviderOptions {
  /** The moli binary: a PATH name or an absolute/relative executable path. */
  binaryPath: string
  /** Budget for one session's server startup (readiness polling) in milliseconds. */
  startupTimeoutMs: number
  /** Budget for one page navigation in milliseconds. */
  navigationTimeoutMs: number
  /** Budget for one CDP command or event wait outside navigation (evaluate, screenshot) in milliseconds. */
  cdpTimeoutMs: number
  /** Character cap on returned page text content. */
  maxContentChars: number
  /** Settle delay after a DOM interaction before the state read, in milliseconds. */
  settleMs: number
  /** Budget for the one-time `--version` availability probe in milliseconds. */
  probeTimeoutMs: number
  /** Interval between readiness polls in milliseconds. */
  pollEveryMs: number
  /** Extra argv appended to the `serve` invocation verbatim (flag overrides). */
  extraServeArgs: readonly string[]
  /** Availability prober; defaults to a real `spawnSync(['--version'])`. */
  prober?: (binaryPath: string, timeoutMs: number) => { status: number | null; error: unknown }
  /** Process spawner; defaults to `node:child_process` spawn with hidden window. */
  spawnFn?: SpawnFn
  /** HTTP fetch used for CDP endpoint discovery; defaults to global `fetch`. */
  fetchFn?: FetchFn
  /** WebSocket constructor used for the CDP connection; defaults to the global. */
  wsFactory?: WebSocketFactory
}
