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
import { CdpConnection, discoverPageTarget } from './cdp.ts'
import { MoliBrowserSession, buildServeArgv, killServeProcess } from './session.ts'
import type { FetchFn, MoliBrowserProviderOptions, SpawnFn, WebSocketFactory } from './types.ts'

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
 * simplicity and state isolation between launches.
 */
export class MoliBrowserProvider implements BrowserProvider {
  readonly id = MOLI_BROWSER_PROVIDER_ID

  private readonly fetchFn: FetchFn
  private readonly spawnFn: SpawnFn
  private readonly wsFactory: WebSocketFactory
  private readonly prober: NonNullable<MoliBrowserProviderOptions['prober']>
  private availability: boolean | undefined

  constructor(private readonly options: MoliBrowserProviderOptions) {
    this.fetchFn = options.fetchFn ?? ((url, init) => fetch(url, init))
    this.spawnFn = options.spawnFn ?? defaultSpawn
    this.wsFactory = options.wsFactory ?? (url => new WebSocket(url))
    this.prober = options.prober ?? defaultProber
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
   *
   * @param signal - aborts startup; the spawned child is killed on abort.
   * @returns the launched session; the caller owns its `close()`.
   */
  async launch(signal?: AbortSignal): Promise<BrowserSession> {
    if (!this.available()) {
      throw new BrowserError(`the moli binary is not usable at "${this.options.binaryPath}"`, 'BROWSER_PROVIDER_UNAVAILABLE')
    }
    const port = await reserveEphemeralPort()
    const child = this.spawnFn(this.options.binaryPath, buildServeArgv({
      port,
      extraServeArgs: this.options.extraServeArgs,
    }))
    try {
      const baseUrl = `http://${SERVE_HOST}:${port}`
      await this.awaitReadiness(baseUrl, signal)
      const target = await discoverPageTarget(this.fetchFn, baseUrl)
      const wsUrl = target.webSocketDebuggerUrl
      if (wsUrl === undefined) {
        throw new BrowserError('the discovered moli CDP target has no debugger URL', 'BROWSER_PROVIDER_ERROR')
      }
      const connection = new CdpConnection(wsUrl, this.wsFactory)
      await connection.open()
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
        maxContentChars: this.options.maxContentChars,
      })
    } catch (error: unknown) {
      killServeProcess(child)
      throw error
    }
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
