/**
 * `MoliBrowserSession`: one `BrowserSession` over an isolated `moli serve`
 * process and its CDP connection. The session owns the full lifecycle — child
 * process, WebSocket, and a sequential-operation mutex — so `close()` from any
 * path tears everything down exactly once.
 *
 * Page text is read through `Runtime.evaluate`; interaction runs small DOM
 * scripts in the page; screenshots ride `Page.captureScreenshot`.
 * @module @deepseek-ai/dsh-browser-moli/session
 */

import { BrowserError } from '@deepseek-ai/dsh-browser'
import type {
  BrowserClickRequest,
  BrowserNavigateRequest,
  BrowserPageState,
  BrowserScreenshot,
  BrowserScreenshotRequest,
  BrowserSession,
  BrowserTypeRequest,
} from '@deepseek-ai/dsh-browser'
import { execFile } from 'node:child_process'
import type { CdpConnection } from './cdp.ts'
import { sessionClosedError } from './cdp.ts'
import type { SpawnedProcess } from './types.ts'

/** One live session's resources: the serve process plus its CDP connection. */
export interface MoliSessionHandle {
  readonly child: SpawnedProcess
  readonly connection: CdpConnection
}

/**
 * Terminate a serve process. On Windows `kill()` does not descend into child
 * processes moli may have spawned, so a best-effort tree kill follows; its
 * failure is swallowed because the session is being discarded either way.
 *
 * @param child - the process to terminate.
 */
export function killServeProcess(child: SpawnedProcess): void {
  child.kill()
  if (process.platform === 'win32' && child.pid !== undefined) {
    execFile('taskkill', ['/pid', String(child.pid), '/t', '/f'], { windowsHide: true }, () => {
      // Best-effort tree kill: when taskkill cannot run the plain kill above
      // already fired, and nothing can recover the process afterwards.
    })
  }
}

/**
 * Build the `moli serve` argv for one session. The default flag name assumes
 * moli's documented CDP endpoint conventions; deployments correct it via
 * `extraServeArgs` without code changes.
 *
 * @param options - the reserved port plus verbatim extra argv.
 * @returns the complete spawn argv after the binary name.
 */
export function buildServeArgv(options: {
  port: number
  extraServeArgs?: readonly string[]
}): string[] {
  return ['serve', '--cdp-port', String(options.port), ...options.extraServeArgs ?? []]
}

/**
 * The moli-backed browser session. Operations serialize through a rolling
 * promise chain — the seam contract makes sessions sequential — and every
 * operation rejects once {@link close} has run. Only the latest chained
 * promise is retained, so finished operations are garbage-collectable.
 */
export class MoliBrowserSession implements BrowserSession {
  private tail: Promise<unknown> = Promise.resolve()
  private closed = false

  constructor(
    private readonly handle: MoliSessionHandle,
    private readonly limits: {
      readonly navigationTimeoutMs: number
      readonly cdpTimeoutMs: number
      readonly maxContentChars: number
      readonly settleMs: number
    },
  ) {}

  /** @inheritDoc */
  async navigate(request: BrowserNavigateRequest, signal?: AbortSignal): Promise<BrowserPageState> {
    const url = assertNavigationUrl(request.url)
    return this.enqueue(async () => {
      // The load-event waiter registers BEFORE the command: a page that
      // finishes loading between command resolution and registration would
      // otherwise miss the event and stall out the whole budget.
      const loaded = this.handle.connection.waitForEvent('Page.loadEventFired', this.limits.navigationTimeoutMs, signal)
      try {
        await this.handle.connection.send('Page.navigate', { url }, this.limits.navigationTimeoutMs, signal)
      } catch (error: unknown) {
        // The send failure is the reported cause; its load-event waiter must
        // not linger until its own deadline or surface as an unhandled rejection.
        loaded.catch(() => {
          // The navigation already failed above; a late timeout/resolution of
          // the abandoned waiter carries no additional information.
        })
        throw error
      }
      await loaded
      return this.readState(signal)
    })
  }

  /** @inheritDoc */
  async snapshot(signal?: AbortSignal): Promise<BrowserPageState> {
    return this.enqueue(() => this.readState(signal))
  }

  /** @inheritDoc */
  async click(request: BrowserClickRequest, signal?: AbortSignal): Promise<BrowserPageState> {
    return this.enqueue(async () => {
      const outcome = await this.evaluateDomScript(`(() => {
        const el = document.querySelector(${JSON.stringify(request.selector)});
        if (!el) return 'missing';
        el.click();
        return 'ok';
      })()`, signal)
      this.assertPresent(outcome)
      return this.settleAndRead(signal)
    })
  }

  /** @inheritDoc */
  async type(request: BrowserTypeRequest, signal?: AbortSignal): Promise<BrowserPageState> {
    return this.enqueue(async () => {
      const submit = request.submit === true
      const outcome = await this.evaluateDomScript(`(() => {
        const el = document.querySelector(${JSON.stringify(request.selector)});
        if (!el) return 'missing';
        const proto = el.constructor.prototype;
        const setter = Object.getOwnPropertyDescriptor(proto, 'value')?.set
          ?? Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
        setter?.call(el, ${JSON.stringify(request.text)});
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
        if (${submit}) {
          for (const type of ['keydown', 'keyup']) {
            el.dispatchEvent(new KeyboardEvent(type, { key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true }));
          }
        }
        return 'ok';
      })()`, signal)
      this.assertPresent(outcome)
      return this.settleAndRead(signal)
    })
  }

  /** @inheritDoc */
  async screenshot(request: BrowserScreenshotRequest, signal?: AbortSignal): Promise<BrowserScreenshot> {
    return this.enqueue(async () => {
      const result = await this.handle.connection.send('Page.captureScreenshot', {
        format: 'png',
        captureBeyondViewport: request.fullPage === true,
      }, this.limits.cdpTimeoutMs, signal) as { data?: string }
      if (result.data === undefined) {
        throw new BrowserError('moli returned no screenshot data', 'BROWSER_CAPTURE_FAILED')
      }
      return {
        mediaType: 'image/png',
        data: new Uint8Array(Buffer.from(result.data, 'base64')),
      }
    })
  }

  /**
   * Release the process and connection. Idempotent: later calls resolve
   * without effect, while queued operations reject with
   * `BROWSER_SESSION_CLOSED`. The connection closes first, which settles
   * every blocked command and event waiter immediately, so the quiescence
   * await below never waits out a deadline timer.
   */
  async close(): Promise<void> {
    if (!this.closed) {
      this.closed = true
      this.handle.connection.close()
      killServeProcess(this.handle.child)
    }
    // The rolling chain transitively covers every operation enqueued before
    // close; each rejects or resolves promptly once the connection is down.
    const pending = this.tail
    await pending.catch(() => {
      // Disposal reports no operation failure; callers of those operations
      // already observed their own rejections.
    })
  }

  /** Serialize one operation behind every prior unresolved operation. */
  private enqueue<T>(operation: () => Promise<T>): Promise<T> {
    if (this.closed) {
      return Promise.reject(sessionClosedError())
    }
    const run = (): Promise<T> => {
      // An operation queued before close() still reaches its turn; the flag
      // makes it fail fast instead of touching the torn-down connection.
      if (this.closed) {
        return Promise.reject(sessionClosedError())
      }
      return operation()
    }
    const current = this.tail.then(run, run)
    this.tail = current
    current.catch(() => {
      // Each caller observes its own rejection through the returned promise;
      // keeping the chain resolved lets later operations still attempt their work.
    })
    return current
  }

  /** Read url/title/body text through three bounded evaluates. */
  private async readState(signal?: AbortSignal): Promise<BrowserPageState> {
    const [url, title, content] = await Promise.all([
      this.evaluateText('location.href', signal),
      this.evaluateText('document.title', signal),
      this.evaluateText(`(document.body?.innerText ?? '').slice(0, ${this.limits.maxContentChars})`, signal),
    ])
    return {
      url,
      ...(title.length > 0 ? { title } : {}),
      ...(content.length > 0 ? { content } : {}),
    }
  }

  /** A short settle delay after DOM interaction before reading state. */
  private async settleAndRead(signal?: AbortSignal): Promise<BrowserPageState> {
    await new Promise(resolve => setTimeout(resolve, this.limits.settleMs))
    return this.readState(signal)
  }

  /** Evaluate one expression returning a string value. */
  private async evaluateText(expression: string, signal?: AbortSignal): Promise<string> {
    const result = await this.handle.connection.send('Runtime.evaluate', {
      expression,
      returnByValue: true,
    }, this.limits.cdpTimeoutMs, signal) as CdpEvaluateResult
    // An in-page exception must surface as a structured error: collapsing it
    // to '' made click/type report success while nothing happened, and
    // snapshot report an empty page.
    if (result.exceptionDetails !== undefined) {
      throw new BrowserError(
        `in-page evaluation failed: ${describeEvaluationFailure(result.exceptionDetails)}`,
        'BROWSER_EVALUATION_FAILED',
      )
    }
    return typeof result.result?.value === 'string' ? result.result.value : ''
  }

  /** Evaluate one DOM-interaction script whose IIFE returns 'ok' or 'missing'. */
  private async evaluateDomScript(expression: string, signal?: AbortSignal): Promise<string> {
    return this.evaluateText(expression, signal)
  }

  /** Throw the structured missing-element error for a failed selector match. */
  private assertPresent(outcome: string): void {
    if (outcome === 'missing') {
      throw new BrowserError('no element matched the given CSS selector', 'BROWSER_ELEMENT_NOT_FOUND')
    }
  }
}

/** The `Runtime.evaluate` response fields this session reads. */
interface CdpEvaluateResult {
  readonly result?: {
    readonly value?: unknown
  }
  readonly exceptionDetails?: {
    readonly text?: string
    readonly exception?: {
      readonly description?: string
    }
  }
}

/** Prefer the exception's own description; fall back to CDP's summary text. */
function describeEvaluationFailure(details: NonNullable<CdpEvaluateResult['exceptionDetails']>): string {
  return details.exception?.description ?? details.text ?? 'unknown in-page error'
}

/**
 * Validate one navigation target against the seam contract (`types.ts`:
 * absolute `http:`/`https:` only, providers reject other schemes) before any
 * CDP traffic carries it.
 *
 * @param url - the raw URL string from the navigate request.
 * @returns the validated original string.
 * @throws BrowserError `BROWSER_INVALID_URL` for unparseable or non-http(s) targets.
 */
function assertNavigationUrl(url: string): string {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch (cause: unknown) {
    throw new BrowserError(`invalid navigation URL: ${url}`, 'BROWSER_INVALID_URL', { cause })
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new BrowserError(`unsupported URL scheme "${parsed.protocol}" (only http and https can be navigated)`, 'BROWSER_INVALID_URL')
  }
  return url
}
