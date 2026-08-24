/**
 * The plugin-owned shared browser session. One lazily-launched
 * {@link BrowserSession} serves every enabled tool in the context; operations
 * serialize behind a promise chain because the seam's sessions are sequential,
 * and the fiber's disposal closes the underlying browser.
 * @module @deepseek-ai/dsh-tool-browser/session-holder
 */

import { BrowserError } from '@deepseek-ai/dsh-browser'
import type { BrowserRuntime, BrowserSession } from '@deepseek-ai/dsh-browser'

/**
 * Lazy, serialized access to one shared browser session. The first operation
 * launches through `ctx.browser`; a failed launch clears itself so the next
 * call retries instead of caching the failure forever.
 */
export class SharedBrowserSession {
  private sessionPromise: Promise<BrowserSession> | undefined
  private tail: Promise<unknown> = Promise.resolve()
  private disposed = false

  constructor(private readonly browser: BrowserRuntime) {}

  /**
   * Run one session operation behind every prior unresolved operation.
   * @param operation - the serialized step receiving the live session.
   * @returns the operation's outcome.
   */
  run<T>(operation: (session: BrowserSession) => Promise<T>): Promise<T> {
    if (this.disposed) {
      return Promise.reject(new BrowserError('the shared browser session is closed', 'BROWSER_SESSION_CLOSED'))
    }
    const previous = this.tail
    const current = previous.then(async () => {
      const session = await this.retain()
      return operation(session)
    })
    this.tail = current.catch(() => {
      // Callers observe their own rejection through `run`; keeping the chain
      // resolved lets later operations attempt their work.
    })
    return current
  }

  /** Close the underlying session exactly once; later operations reject. */
  async dispose(): Promise<void> {
    this.disposed = true
    const session = this.sessionPromise
    this.sessionPromise = undefined
    try {
      await this.tail
    } catch {
      // Disposal follows the last observed operation regardless of its outcome.
    }
    if (session !== undefined) {
      const settled = await session.then(
        value => value,
        () => undefined,
      )
      await settled?.close()
    }
  }

  /** Launch once; a rejected launch is forgotten so the next call retries. */
  private retain(): Promise<BrowserSession> {
    if (this.sessionPromise === undefined) {
      const launching = this.browser.launch().catch((error: unknown) => {
        if (this.sessionPromise === launching) this.sessionPromise = undefined
        throw error
      })
      this.sessionPromise = launching
    }
    return this.sessionPromise
  }
}
