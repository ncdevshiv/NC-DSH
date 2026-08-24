/**
 * `MoliBrowserProvider`: a `BrowserProvider` over the local
 * [moli](https://github.com/lexmount/moli) headless browser. Each launch spawns
 * an isolated `moli serve` process on a reserved ephemeral port, waits for its
 * HTTP CDP endpoint to answer, attaches to a page target, and hands out the
 * session. Availability is a memoized local `--version` probe.
 * @module @deepseek-ai/dsh-browser-moli/provider
 */

import { BrowserError } from '@deepseek-ai/dsh-browser'
import type { BrowserProvider, BrowserSession } from '@deepseek-ai/dsh-browser'
import { spawn as nodeSpawn } from 'node:child_process'
import { createServer } from 'node:net'
import type { AddressInfo } from 'node:net'
import { spawnSync } from 'node:child_process'
import type { CdpTarget } from './cdp.ts'
import { CdpConnection, discoverPageTarget } from './cdp.ts'
import { MoliBrowserSession, buildServeArgv, killServeProcess } from './session.ts'
import type { FetchFn, MoliBrowserProviderOptions, SpawnFn, SpawnedProcess, WebSocketFactory } from './types.ts'

/** Stable id this provider registers under. */
export const MOLI_BROWSER_PROVIDER_ID = 'moli'

/** The loopback host every serve process binds. */
const SERVE_HOST = '127.0.0.1'

/**
 * Reserve an ephemeral TCP port by binding once and releasing. A race window
 * remains between release and the child's bind; readiness polling is the
 * arbiter that fails loud when the port is lost.
 *
 * @returns the reserved port number.
 */
export async function reserveEphemeralPort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer()
    server.once('error', reject)
    server.listen(0, SERVE_HOST, () => {
      const { port } = server.address() as AddressInfo
      server.close(() => { resolve(port) })
    })
  })
}

/**
 * The moli-backed browser provider. One provider instance may hand out many
 * sessions; each session owns an isolated serve process for teardown
 * simplicity and state isolation between launches. Live serve processes are
 * tracked so Node's synchronous exit phase can force-kill any the host leaves
 * behind; graceful session `close()` remains the caller's ownership.
 */
export class MoliBrowserProvider implements BrowserProvider {
  readonly id = MOLI_BROWSER_PROVIDER_ID

  private readonly fetchFn: FetchFn
  private readonly spawnFn: SpawnFn
  private readonly wsFactory: WebSocketFactory
  private readonly prober: NonNullable<MoliBrowserProviderOptions['prober']>
  /** Serve processes of sessions not yet closed, for host-exit finalization. */
  private readonly live = new Set<SpawnedProcess>()
  private availability: boolean | undefined

  constructor(private readonly options: MoliBrowserProviderOptions) {
    this.fetchFn = options.fetchFn ?? ((url, init) => fetch(url, init))
    this.spawnFn = options.spawnFn ?? defaultSpawn
    this.wsFactory = options.wsFactory ?? (url => new WebSocket(url))
    this.prober = options.prober ?? defaultProber
  }

  /**
   * Force-kill every tracked live serve process. Called during Node's
   * synchronous exit phase, where awaiting or reporting is impossible; a
   * target that cannot be killed must not stop the remaining kills.
   */
  terminateForHostExit(): void {
    for (const child of this.live) {
      try {
        killServeProcess(child)
      } catch (_hostExitCannotAwaitOrRecoverOneTarget) {
        // Host exit cannot await or report one target; continue with the rest.
      }
    }
    this.live.clear()
  }

  /**
   * Register the synchronous host-exit finalization for this provider's live
   * serve processes.
   * @returns the disposer that detaches the listener.
   */
  installHostExitFinalization(): () => void {
    const onHostExit = (): void => { this.terminateForHostExit() }
    process.prependListener('exit', onHostExit)
    return () => {
      process.off('exit', onHostExit)
    }
  }

  /** Track one spawned child until its kill; removal keeps the set authoritative. */
  private track(child: SpawnedProcess): SpawnedProcess {
    const tracked: SpawnedProcess = {
      pid: child.pid,
      kill: (): void => {
        this.live.delete(tracked)
        killServeProcess(child)
      },
    }
    this.live.add(tracked)
    return tracked
  }

  /**
   * Cheap local usability check: the configured binary exists and runs. The
   * probe runs at most once per provider instance.
   * @returns true when `moli --version` exited successfully.
   */
  available(): boolean {
    if (this.availability === undefined) {
      const probe = this.prober(this.options.binaryPath, this.options.probeTimeoutMs)
      this.availability = probe.status === 0 && probe.error === null
    }
    return this.availability
  }

  /**
   * Launch one isolated browser session: reserve a port, spawn `moli serve`,
   * wait for CDP readiness, attach to a page target, and enable the Page
   * domain. Any failure after the spawn tears the child down before throwing.
   * Discovery and the WebSocket open share the startup budget and the abort
   * signal, so neither phase can stall past it.
   *
   * @param signal - aborts startup; the spawned child is killed on abort.
   * @returns the launched session; the caller owns its `close()`.
   */
  async launch(signal?: AbortSignal): Promise<BrowserSession> {
    if (!this.available()) {
      throw new BrowserError(`the moli binary is not usable at "${this.options.binaryPath}"`, 'BROWSER_PROVIDER_UNAVAILABLE')
    }
    const port = await reserveEphemeralPort()
    const child = this.track(this.spawnFn(this.options.binaryPath, buildServeArgv({
      port,
      extraServeArgs: this.options.extraServeArgs,
    })))
    // Discovery and open ride the same startup budget as readiness polling:
    // without it, a hung fetch or WebSocket open stalls launch indefinitely.
    const startupScope = signal !== undefined
      ? AbortSignal.any([signal, AbortSignal.timeout(this.options.startupTimeoutMs)])
      : AbortSignal.timeout(this.options.startupTimeoutMs)
    try {
      const baseUrl = `http://${SERVE_HOST}:${port}`
      await this.awaitReadiness(baseUrl, signal)
      let target: CdpTarget
      try {
        target = await discoverPageTarget(this.fetchFn, baseUrl, startupScope)
      } catch (error: unknown) {
        throw this.classifyStartupScopeFailure(error, signal, startupScope)
      }
      const wsUrl = target.webSocketDebuggerUrl
      if (wsUrl === undefined) {
        throw new BrowserError('the discovered moli CDP target has no debugger URL', 'BROWSER_PROVIDER_ERROR')
      }
      const connection = new CdpConnection(wsUrl, this.wsFactory)
      const opened = await this.openWithinStartupScope(connection, startupScope)
      if (!opened) {
        connection.close()
        throw this.classifyStartupScopeFailure(new Error('the WebSocket open did not settle within the startup budget'), signal, startupScope)
      }
      try {
        await connection.send('Page.enable')
      } catch (error: unknown) {
        connection.close()
        throw error
      }
      return new MoliBrowserSession({
        child,
        connection,
      }, {
        navigationTimeoutMs: this.options.navigationTimeoutMs,
        cdpTimeoutMs: this.options.cdpTimeoutMs,
        maxContentChars: this.options.maxContentChars,
        settleMs: this.options.settleMs,
      })
    } catch (error: unknown) {
      child.kill()
      throw error
    }
  }

  /**
   * Resolve `true` when the connection opened first, `false` once the startup
   * scope expired while open was still pending. A genuine open failure
   * rejects with its own error; the scope listener detaches on every settle.
   */
  private openWithinStartupScope(connection: CdpConnection, startupScope: AbortSignal): Promise<boolean> {
    if (startupScope.aborted) {
      return Promise.resolve(false)
    }
    return new Promise((resolve, reject) => {
      const onScopeAbort = (): void => {
        resolve(false)
      }
      const detach = (): void => {
        startupScope.removeEventListener('abort', onScopeAbort)
      }
      startupScope.addEventListener('abort', onScopeAbort, { once: true })
      connection.open().then(
        () => {
          detach()
          resolve(true)
        },
        (error: unknown) => {
          detach()
          reject(error)
        },
      )
    })
  }

  /** Map a discovery/open failure inside the startup scope to abort or timeout. */
  private classifyStartupScopeFailure(error: unknown, signal: AbortSignal | undefined, startupScope: AbortSignal): BrowserError {
    if (signal?.aborted) {
      return new BrowserError('the moli browser session launch was aborted', 'BROWSER_ABORTED', { cause: error })
    }
    if (startupScope.aborted) {
      return new BrowserError(`the moli serve endpoint did not become ready within ${this.options.startupTimeoutMs}ms`, 'BROWSER_STARTUP_TIMEOUT', { cause: error })
    }
    return error instanceof BrowserError ? error : new BrowserError(`moli CDP target discovery failed: ${String(error)}`, 'BROWSER_PROVIDER_ERROR', { cause: error })
  }

  /**
   * Poll `/json/version` until the endpoint answers or the budget/abort wins.
   * Each cycle races one probe attempt against the poll interval, so a hung
   * connection can never stall the abort or deadline decisions.
   *
   * @param baseUrl - the serve process's HTTP root.
   * @param signal - caller abort during startup.
   */
  private async awaitReadiness(baseUrl: string, signal?: AbortSignal): Promise<void> {
    const deadline = Date.now() + this.options.startupTimeoutMs
    for (;;) {
      if (wasAborted(signal)) {
        throw new BrowserError('the moli browser session launch was aborted', 'BROWSER_ABORTED')
      }
      const ready = await Promise.race([this.probeOnce(baseUrl), tick(this.options.pollEveryMs, signal)])
      if (ready) return
      if (wasAborted(signal)) {
        throw new BrowserError('the moli browser session launch was aborted', 'BROWSER_ABORTED')
      }
      if (Date.now() >= deadline) {
        throw new BrowserError(`the moli serve endpoint did not become ready within ${this.options.startupTimeoutMs}ms`, 'BROWSER_STARTUP_TIMEOUT')
      }
    }
  }

  /** One readiness attempt: true only when `/json/version` answers OK. */
  private async probeOnce(baseUrl: string): Promise<boolean> {
    try {
      const response = await this.fetchFn(`${baseUrl}/json/version`)
      return response.ok
    } catch {
      // The endpoint is not accepting connections yet — keep polling until
      // the deadline decides; the child may still be booting.
      return false
    }
  }
}

/** True when the launch signal has fired; a function call so TS cannot narrow the live flag. */
function wasAborted(signal: AbortSignal | undefined): boolean {
  return signal?.aborted === true
}

/**
 * A delay that resolves early when aborted, so every poll cycle regains
 * control promptly instead of waiting out the full interval.
 */
async function tick(ms: number, signal?: AbortSignal): Promise<undefined> {
  return new Promise((resolve) => {
    const timer = setTimeout(done, ms)
    function done(): void {
      clearTimeout(timer)
      signal?.removeEventListener('abort', done)
      resolve(undefined)
    }
    signal?.addEventListener('abort', done, { once: true })
  })
}

/** Real spawn boundary: hidden window, detached stdio so pipes never block. */
function defaultSpawn(binaryPath: string, args: readonly string[]): { pid: number | undefined; kill(): void } {
  const child = nodeSpawn(binaryPath, [...args], { stdio: 'ignore', windowsHide: true })
  return {
    pid: child.pid,
    kill: () => void child.kill(),
  }
}

/** Default one-shot probe of the moli binary. */
function defaultProber(binaryPath: string, timeoutMs: number): { status: number | null; error: unknown } {
  const result = spawnSync(binaryPath, ['--version'], { timeout: timeoutMs, windowsHide: true })
  return { status: result.status, error: result.error ?? null }
}

export { buildServeArgv, killServeProcess }
export type { FetchFn, MoliBrowserProviderOptions, SpawnFn, WebSocketFactory } from './types.ts'
