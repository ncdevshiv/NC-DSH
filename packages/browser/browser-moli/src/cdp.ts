/**
 * Minimal CDP (Chrome DevTools Protocol) client over WebSocket for the moli
 * automation endpoint: id-correlated command/response, event waiters, and the
 * HTTP target discovery the `/json` endpoints provide. Node's global
 * `WebSocket` carries the connection; no dependency is added.
 * @module @deepseek-ai/dsh-browser-moli/cdp
 */

import { BrowserError } from '@deepseek-ai/dsh-browser'

/** One discovered CDP page target from `/json/list`. */
export interface CdpTarget {
  readonly type: string
  readonly webSocketDebuggerUrl?: string
}

/**
 * Discover a page target on an HTTP CDP endpoint, creating one when none
 * exists. Prefers an existing `page` target; otherwise issues the
 * `PUT /json/new` create request.
 *
 * @param fetchFn - the fetch boundary.
 * @param baseUrl - the endpoint root, e.g. `http://127.0.0.1:9222`.
 * @param signal - cancellation carried into both discovery fetches.
 * @returns the discovered target with its WebSocket debugger URL.
 * @throws BrowserError `BROWSER_PROVIDER_ERROR` when discovery fails or no
 *   debugger URL is reachable.
 */
export async function discoverPageTarget(
  fetchFn: (url: string, init?: { method?: string; signal?: AbortSignal }) => Promise<Response>,
  baseUrl: string,
  signal?: AbortSignal,
): Promise<CdpTarget> {
  let targets: CdpTarget[]
  try {
    const response = await fetchFn(`${baseUrl}/json/list`, { ...signal !== undefined ? { signal } : {} })
    if (!response.ok) {
      throw new BrowserError(`moli CDP target listing answered HTTP ${response.status}`, 'BROWSER_PROVIDER_ERROR')
    }
    const body: unknown = await response.json()
    if (!Array.isArray(body)) {
      throw new BrowserError('moli CDP target listing returned a non-array body', 'BROWSER_PROVIDER_ERROR')
    }
    targets = body as CdpTarget[]
  } catch (error: unknown) {
    if (error instanceof BrowserError) throw error
    throw new BrowserError(`moli CDP target discovery failed: ${String(error)}`, 'BROWSER_PROVIDER_ERROR', { cause: error })
  }
  const existing = targets.find(target => target.type === 'page' && target.webSocketDebuggerUrl !== undefined)
  if (existing !== undefined) return existing
  try {
    const created = await fetchFn(`${baseUrl}/json/new?about:blank`, {
      method: 'PUT',
      ...signal !== undefined ? { signal } : {},
    })
    if (!created.ok) {
      throw new BrowserError(`moli CDP target creation answered HTTP ${created.status}`, 'BROWSER_PROVIDER_ERROR')
    }
    const target = await created.json() as CdpTarget
    if (target.webSocketDebuggerUrl === undefined) {
      throw new BrowserError('moli CDP target creation returned no debugger URL', 'BROWSER_PROVIDER_ERROR')
    }
    return target
  } catch (error: unknown) {
    if (error instanceof BrowserError) throw error
    throw new BrowserError(`moli CDP target creation failed: ${String(error)}`, 'BROWSER_PROVIDER_ERROR', { cause: error })
  }
}

interface Pending {
  resolve: (value: unknown) => void
  reject: (error: unknown) => void
  timer: ReturnType<typeof setTimeout>
  /** Detaches the caller's abort listener; present only when a signal was given. */
  removeAbort?: () => void
}

interface EventWaiter {
  resolve: () => void
  reject: (error: unknown) => void
  timer: ReturnType<typeof setTimeout>
  /** Detaches the caller's abort listener; present only when a signal was given. */
  removeAbort?: () => void
}

/**
 * One CDP WebSocket connection. Commands correlate by monotonically assigned
 * ids; a caller may wait for the next named protocol event with a deadline.
 * After {@link close}, every blocked command and event waiter rejects with
 * `BROWSER_SESSION_CLOSED`, and later calls reject immediately.
 */
export class CdpConnection {
  private nextId = 1
  private isClosed = false
  private readonly pending = new Map<number, Pending>()
  private readonly eventWaiters = new Map<string, EventWaiter[]>()
  private readonly ws: WebSocket

  constructor(
    wsUrl: string,
    wsFactory: (url: string) => WebSocket = url => new WebSocket(url),
  ) {
    this.ws = wsFactory(wsUrl)
    this.ws.addEventListener('message', (event) => { this.onMessage(String(event.data)) })
  }

  /** Resolve when the socket is open; reject on open failure or early close. */
  open(): Promise<void> {
    return new Promise((resolve, reject) => {
      const settleOpen = (): void => {
        this.ws.removeEventListener('open', onOpen)
        this.ws.removeEventListener('error', onError)
        this.ws.removeEventListener('close', onClose)
      }
      const onOpen = (): void => {
        settleOpen()
        resolve()
      }
      const onError = (event: Event): void => {
        settleOpen()
        reject(new BrowserError('moli CDP connection failed to open', 'BROWSER_PROVIDER_ERROR', { cause: event }))
      }
      const onClose = (): void => {
        settleOpen()
        reject(new BrowserError('moli CDP connection closed before opening', 'BROWSER_PROVIDER_ERROR'))
      }
      this.ws.addEventListener('open', onOpen, { once: true })
      this.ws.addEventListener('error', onError, { once: true })
      this.ws.addEventListener('close', onClose, { once: true })
    })
  }

  /**
   * Send one CDP command and resolve with its `result`.
   * @param method - the protocol method, e.g. `Page.navigate`.
   * @param params - the method parameters.
   * @param timeoutMs - response deadline; a silent endpoint rejects past it.
   * @param signal - caller cancellation; an aborted signal rejects the call
   *   as `BROWSER_ABORTED` and stops tracking its response.
   * @returns the command's `result` payload.
   */
  send(method: string, params: Record<string, unknown> = {}, timeoutMs = 30_000, signal?: AbortSignal): Promise<unknown> {
    if (this.isClosed) {
      return Promise.reject(sessionClosedError())
    }
    if (signal?.aborted) {
      return Promise.reject(operationAbortedError())
    }
    const id = this.nextId++
    return new Promise((resolve, reject) => {
      const onAbort = (): void => {
        this.pending.delete(id)
        clearTimeout(timer)
        reject(operationAbortedError())
      }
      const removeAbort = (): void => {
        signal?.removeEventListener('abort', onAbort)
      }
      const timer = setTimeout(() => {
        this.pending.delete(id)
        removeAbort()
        reject(new BrowserError(`moli CDP call ${method} timed out`, 'BROWSER_PROVIDER_ERROR'))
      }, timeoutMs)
      this.pending.set(id, {
        resolve,
        reject,
        timer,
        removeAbort,
      })
      signal?.addEventListener('abort', onAbort, { once: true })
      try {
        this.ws.send(JSON.stringify({
          id,
          method,
          params,
        }))
      } catch (error: unknown) {
        // A socket that died between the open handshake and this write rejects
        // as a provider error with the transport cause, never as a raw
        // DOMException escaping the executor.
        this.pending.delete(id)
        clearTimeout(timer)
        removeAbort()
        reject(new BrowserError(`moli CDP call ${method} could not be sent`, 'BROWSER_PROVIDER_ERROR', { cause: error }))
      }
    })
  }

  /**
   * Resolve on the next occurrence of a protocol event, or reject at the deadline.
   * @param method - the event name, e.g. `Page.loadEventFired`.
   * @param timeoutMs - waiting deadline in milliseconds.
   * @param signal - caller cancellation; an aborted signal rejects the wait
   *   as `BROWSER_ABORTED` and removes its waiter.
   */
  waitForEvent(method: string, timeoutMs: number, signal?: AbortSignal): Promise<void> {
    if (this.isClosed) {
      return Promise.reject(sessionClosedError())
    }
    if (signal?.aborted) {
      return Promise.reject(operationAbortedError())
    }
    return new Promise((resolve, reject) => {
      const queue = this.eventWaiters.get(method) ?? []
      const onAbort = (): void => {
        this.forgetWaiter(method, waiter)
        clearTimeout(waiter.timer)
        reject(operationAbortedError())
      }
      const removeAbort = (): void => {
        signal?.removeEventListener('abort', onAbort)
      }
      const waiter: EventWaiter = {
        resolve,
        reject,
        timer: setTimeout(() => {
          removeAbort()
          this.forgetWaiter(method, waiter)
          reject(new BrowserError(`timed out waiting for moli CDP event ${method}`, 'BROWSER_NAVIGATION_TIMEOUT'))
        }, timeoutMs),
        removeAbort,
      }
      queue.push(waiter)
      this.eventWaiters.set(method, queue)
      signal?.addEventListener('abort', onAbort, { once: true })
    })
  }

  /**
   * Close the underlying socket. Pending commands and outstanding event
   * waiters reject as session-closed immediately — without this, a caller
   * blocked in {@link waitForEvent} would hold its deadline open past teardown.
   */
  close(): void {
    if (!this.isClosed) {
      this.isClosed = true
      try {
        this.ws.close()
      } catch {
        // A half-open socket must not mask the teardown that follows; closing
        // again during process shutdown is harmless and unrecoverable either way.
      }
    }
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer)
      pending.removeAbort?.()
      pending.reject(sessionClosedError())
    }
    this.pending.clear()
    for (const queue of this.eventWaiters.values()) {
      for (const waiter of queue.splice(0)) {
        clearTimeout(waiter.timer)
        waiter.removeAbort?.()
        waiter.reject(sessionClosedError())
      }
    }
    this.eventWaiters.clear()
  }

  private onMessage(raw: string): void {
    let parsed: unknown
    try {
      parsed = JSON.parse(raw)
    } catch {
      // A non-JSON frame cannot be correlated or routed; dropping it loses
      // nothing recoverable — the sender treats silence as its own timeout.
      return
    }
    if (typeof parsed !== 'object' || parsed === null) return
    const message = parsed as { id?: number; method?: string; error?: unknown; result?: unknown }
    if (message.id !== undefined) {
      const pending = this.pending.get(message.id)
      if (pending === undefined) return
      this.pending.delete(message.id)
      clearTimeout(pending.timer)
      pending.removeAbort?.()
      if (message.error !== undefined) {
        pending.reject(new BrowserError(`moli CDP call failed: ${JSON.stringify(message.error)}`, 'BROWSER_PROVIDER_ERROR'))
      } else {
        pending.resolve(message.result)
      }
      return
    }
    if (message.method !== undefined) {
      const queue = this.eventWaiters.get(message.method)
      if (queue === undefined) return
      this.eventWaiters.delete(message.method)
      for (const waiter of queue.splice(0)) {
        clearTimeout(waiter.timer)
        waiter.removeAbort?.()
        waiter.resolve()
      }
    }
  }

  /** Drop one waiter from its event queue; a no-op once close() cleared it. */
  private forgetWaiter(method: string, waiter: EventWaiter): void {
    const queue = this.eventWaiters.get(method)
    if (queue === undefined) return
    const index = queue.indexOf(waiter)
    if (index >= 0) queue.splice(index, 1)
    if (queue.length === 0) this.eventWaiters.delete(method)
  }
}

/** The one rejection every blocked call sees after teardown. */
export function sessionClosedError(): BrowserError {
  return new BrowserError('the moli browser session was closed', 'BROWSER_SESSION_CLOSED')
}

/** The rejection a caller-cancelled CDP operation sees. */
export function operationAbortedError(): BrowserError {
  return new BrowserError('the moli browser operation was aborted', 'BROWSER_ABORTED')
}
