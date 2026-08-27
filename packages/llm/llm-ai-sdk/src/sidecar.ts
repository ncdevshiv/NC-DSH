/**
 * One long-lived `ai-sidecar` child process and its JSON-RPC client. The
 * child speaks newline-delimited JSON-RPC 2.0 over stdio (the shared
 * `JsonRpcLineTransport`); this module owns spawning, the lazy initialize,
 * provider configuration generations, stream multiplexing by `stream_id`,
 * and whole-process disposal.
 * @module @deepseek-ai/dsh-llm-ai-sdk/sidecar
 */

import { spawn } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { JsonRpcLineTransport, JsonRpcResponseError } from '@deepseek-ai/dsh-sdk-protocol'
import { timeoutOf } from '@deepseek-ai/dsh-timeout'
import type { WireChatRequest } from './translate.ts'
import type {
  SidecarApiKind,
  SidecarConfigureParams,
  SidecarDiscoverParams,
  SidecarDiscoveredModel,
  SidecarProviderProfile,
  SidecarStreamEvent,
} from './types.ts'

/** How long one sidecar request may take before the call fails. */
export const DEFAULT_REQUEST_TIMEOUT_MS = 120_000

/** The idle-watchdog code a stalled read recognizes on its abort signal. */
export const STREAM_IDLE_TIMEOUT_CODE = 'LLM_STREAM_IDLE_TIMEOUT'

interface StreamState {
  readonly buffer: SidecarStreamEvent[]
  finished: boolean
  failure: Error | undefined
  wake: (() => void) | undefined
}

/**
 * Fail-loud resolution facts for one sidecar connection. The plugin resolves
 * them once per operation so configuration changes reach the next request
 * without re-registration.
 */
export interface SidecarConnection {
  /** Executable to launch (`ai-sidecar`, or a host runtime wrapping it). */
  command: string
  /** Arguments appended after {@link command}; empty for a native binary. */
  args: readonly string[]
}

/** One streamed completion as an async iterable of sidecar events. */
export interface SidecarStream extends AsyncIterable<SidecarStreamEvent> {}

/**
 * A sidecar round trip failed at the protocol layer: spawn failure, child
 * exit, request ceiling, or a JSON-RPC error response. `kind` carries the
 * sidecar's typed error kind verbatim when one answered; `retryable` rides
 * the sidecar's own classification.
 */
export class SidecarProtocolError extends Error {
  constructor(
    readonly kind: string | undefined,
    message: string,
    readonly retryable?: boolean,
    cause?: unknown,
  ) {
    super(message, cause === undefined ? undefined : { cause })
    this.name = 'SidecarProtocolError'
  }
}

/**
 * One long-lived sidecar child and its JSON-RPC client. Lifecycle ownership is
 * total: spawn, initialize, configure, stream, discover, and dispose all run
 * through here, and every exit path terminates the child.
 */
export class AiSidecarClient {
  private child: ReturnType<typeof spawn> | undefined
  private transport: JsonRpcLineTransport | undefined
  private initialized: Promise<void> | undefined
  private nextRequestId = 0
  private readonly streams = new Map<string, StreamState>()
  private readonly pendingTimeouts = new Map<number, NodeJS.Timeout>()
  private disposed = false

  constructor(private readonly resolveConnection: () => SidecarConnection) {}

  /**
   * OS process id of the live sidecar child, or `undefined` while no child is
   * running. Diagnostics and tests observe the lifecycle through this instead
   * of reaching into the private handle.
   */
  get pid(): number | undefined {
    return this.child?.pid
  }

  /**
   * Spawn (if needed), initialize, and return the transport. Idempotent;
   * concurrent callers share one startup.
   */
  private async start(): Promise<JsonRpcLineTransport> {
    if (this.disposed) {
      throw new SidecarProtocolError(undefined, 'llm-ai-sdk: sidecar client is disposed')
    }
    if (this.transport !== undefined && this.child !== undefined && this.child.exitCode === null) {
      return this.transport
    }
    this.initialized ??= this.spawnAndInitialize()
    await this.initialized
    if (this.transport === undefined) {
      throw new SidecarProtocolError(undefined, 'llm-ai-sdk: sidecar failed to provide a transport')
    }
    return this.transport
  }

  private spawnAndInitialize(): Promise<void> {
    const connection = this.resolveConnection()
    let child: ReturnType<typeof spawn>
    try {
      child = spawn(connection.command, [...connection.args], {
        stdio: ['pipe', 'pipe', 'pipe'],
        windowsHide: true,
      })
    } catch (error: unknown) {
      this.initialized = undefined
      throw new SidecarProtocolError(
        undefined, `llm-ai-sdk: cannot launch ai-sidecar at "${connection.command}"`, undefined, error,
      )
    }
    // The child is retained before any await so `dispose()` interleaving with
    // this startup always sees it; a lost reference here would orphan the
    // process for the lifetime of the machine.
    this.child = child
    let launchFailure: SidecarProtocolError | undefined
    child.once('error', () => {
      /* Spawn failures (ENOENT, permissions) emit `error` without `exit`. The
         launch wording is captured so the pending initialize rejects with the
         cause instead of the transport-close noise the teardown produces. */
      launchFailure = new SidecarProtocolError(
        undefined, `llm-ai-sdk: cannot launch ai-sidecar at "${connection.command}"`,
      )
      this.onChildGone(child)
    })
    const exited = new Promise<never>((_resolve, reject) => {
      child.once('exit', (code, signal) => {
        this.onChildGone(child)
        reject(new SidecarProtocolError(undefined, `llm-ai-sdk: ai-sidecar exited unexpectedly (code ${code}, signal ${signal ?? 'none'})` ))
      })
    })
    exited.catch(() => {})

    const stdout = child.stdout ?? process.stdout
    const stdin = child.stdin ?? process.stdin
    const transport = new JsonRpcLineTransport(stdout, stdin)
    transport.onNotification((method, params) => { this.onNotification(method, params) })
    transport.start()
    this.transport = transport
    return this.request('initialize', {}).then(
      () => {},
      (error: unknown) => {
        /* A failed initialize leaves a healthy process behind (request
           timeout, JSON-RPC error response): tear its generation down so the
           next attempt spawns fresh instead of stacking a second sidecar
           beside this one. An already-gone child tore itself down through its
           own exit handler; a launch failure reports its own wording. */
        if (this.child === child) this.onChildGone(child)
        throw launchFailure ?? error
      },
    )
  }

  /**
   * Idempotent teardown shared by child exit, spawn failure, initialize
   * failure, and disposal: close the transport, fail every stream, and kill
   * any still-running child so no generation outlives its client. A `gone`
   * generation that is no longer the current one is ignored — its kill fires
   * this again after a successor has spawned, and that newer transport must
   * survive.
   */
  private onChildGone(gone: ReturnType<typeof spawn> | undefined): void {
    const child = this.child
    if (gone !== undefined && child !== gone) return
    this.transport?.close()
    this.transport = undefined
    this.initialized = undefined
    this.failAllStreams(
      new SidecarProtocolError('network', 'llm-ai-sdk: ai-sidecar exited before completing its streams', true),
    )
    this.child = undefined
    if (child === undefined || child.exitCode !== null) return
    try {
      child.kill()
    } catch {
      /* An already-dead child owns no cleanup here; exit observation is complete. */
    }
  }

  private onNotification(method: string, params: Record<string, unknown>): void {
    if (method !== 'chat/event' && method !== 'chat/done') return
    const streamId = typeof params.stream_id === 'string' ? params.stream_id : undefined
    if (streamId === undefined) return
    const state = this.streams.get(streamId)
    if (state === undefined) return
    if (method === 'chat/event') {
      state.buffer.push(params.event as SidecarStreamEvent)
    } else {
      state.finished = true
      if (params.ok !== true && state.failure === undefined) {
        const error = (params.error ?? {}) as { message?: string; kind?: string; retryable?: boolean }
        state.failure = new SidecarProtocolError(error.kind, error.message ?? 'llm-ai-sdk: sidecar stream failed', error.retryable )
      }
    }
    state.wake?.()
    state.wake = undefined
  }

  /**
   * Send one request with the configured timeout.
     A JSON-RPC error response rejects with {@link SidecarProtocolError} carrying
   * the sidecar's typed error kind.
   */
  private async request(method: string, params: object, signal?: AbortSignal): Promise<unknown> {
    if (signal?.aborted) {
      throw new SidecarProtocolError('cancelled', 'llm-ai-sdk: sidecar request cancelled', false, signal.reason)
    }
    const transport = await this.start()
    const id = ++this.nextRequestId
    const abortOnSignal = signal === undefined
      ? undefined
      : new Promise<never>((_resolve, reject) => {
        signal.addEventListener('abort', () => {
          reject(new SidecarProtocolError('cancelled', 'llm-ai-sdk: sidecar request cancelled', false, signal.reason))
        }, { once: true })
      })
    try {
      const racers: Promise<unknown>[] = [transport.request(method, params), this.childExitOrTimeout(id)]
      if (abortOnSignal !== undefined) racers.push(abortOnSignal)
      const result = await Promise.race(racers)
      // Success path clears its timeout so the map doesn't leak.
      const timer = this.pendingTimeouts.get(id)
      if (timer !== undefined) {
        clearTimeout(timer)
        this.pendingTimeouts.delete(id)
      }
      return result
    } catch (error: unknown) {
      const timer = this.pendingTimeouts.get(id)
      if (timer !== undefined) {
        clearTimeout(timer)
        this.pendingTimeouts.delete(id)
      }
      if (error instanceof JsonRpcResponseError) {
        const data = (error.data ?? {}) as { kind?: string; retryable?: boolean }
        throw new SidecarProtocolError(data.kind, error.message, data.retryable, error)
      }
      throw error
    }
  }

  /** Timeout race partner; child exit or spawn failure rejects immediately. Per-request timer map survives swap. */
  private childExitOrTimeout(id: number): Promise<never> {
    return new Promise((_resolve, reject) => {
      const child = this.child
      const timer: NodeJS.Timeout = setTimeout(
        () => {
          this.pendingTimeouts.delete(id)
          if (child !== undefined) {
            child.off('exit', onExit)
            child.off('error', onError)
          }
          reject(new SidecarProtocolError('timeout', 'llm-ai-sdk: sidecar request timed out', true))
        },
        DEFAULT_REQUEST_TIMEOUT_MS,
      )
      this.pendingTimeouts.set(id, timer)
      const onExit = (): void => {
        const t = this.pendingTimeouts.get(id)
        if (t !== undefined) {
          clearTimeout(t)
          this.pendingTimeouts.delete(id)
        }
        reject(new SidecarProtocolError(undefined, 'llm-ai-sdk: ai-sidecar exited mid-request'))
      }
      const onError = (): void => {
        const t = this.pendingTimeouts.get(id)
        if (t !== undefined) {
          clearTimeout(t)
          this.pendingTimeouts.delete(id)
        }
        reject(new SidecarProtocolError(undefined, 'llm-ai-sdk: ai-sidecar failed to launch'))
      }
      timer.unref()
      if (child !== undefined) {
        child.once('exit', onExit)
        // A failed spawn emits `error` without `exit`; without this the request
        // would hang until the ceiling instead of failing at launch.
        child.once('error', onError)
      } else {
        // No child yet — expire immediately rather than hanging until the 120s
        // ceiling; the caller's `start()` race will still surface the real cause.
        clearTimeout(timer)
        this.pendingTimeouts.delete(id)
        reject(new SidecarProtocolError(undefined, 'llm-ai-sdk: ai-sidecar failed to launch'))
      }
    })
  }

  /**
   * Push one provider-configuration generation to the child. The caller
   * decides when facts changed; the child replaces its whole client.
   * @param providers - resolved profiles keyed by route id.
   * @param defaultProvider - route used for references without a prefix.
   * @returns nothing; a refusal throws typed {@link SidecarProtocolError}.
   */
  async configure(providers: Record<string, SidecarProviderProfile>, defaultProvider?: string): Promise<void> {
    const params: SidecarConfigureParams = {
      providers,
      ...(defaultProvider === undefined ? {} : { default_provider: defaultProvider }),
    }
    await this.request('configure', params)
  }

  /**
   * Ask one endpoint that configuration has not stored yet which models it
   * advertises. The interrogation builds a transient provider inside the
   * child and never touches the configured generation, so a draft being
   * probed cannot disturb in-flight streams. The key crosses this call only
   * and is held by nothing on either side afterwards.
   * @param request - the draft's endpoint facts, in harness camelCase.
   * @returns the endpoint-reported model rows, untranslated.
   */
  async discoverModels(
    request: { apiKey?: string; baseURL?: string; api?: SidecarApiKind },
    signal?: AbortSignal,
  ): Promise<SidecarDiscoveredModel[]> {
    const params: SidecarDiscoverParams = {
      ...(request.apiKey === undefined ? {} : { api_key: request.apiKey }),
      ...(request.baseURL === undefined ? {} : { base_url: request.baseURL }),
      ...(request.api === undefined ? {} : { api: request.api }),
    }
    const result = (await this.request('model.discover', params, signal)) as {
      models?: readonly unknown[]
    }
    const rows = result.models ?? []
    const out: SidecarDiscoveredModel[] = []
    for (const row of rows) {
      if (row === null || typeof row !== 'object') continue
      const candidateId = (row as { id?: unknown }).id
      if (typeof candidateId !== 'string' || candidateId.length === 0) continue
      out.push(row as SidecarDiscoveredModel)
    }
    return out
  }

  /** Provider ids currently registered inside the child.
   * @returns the configured generation's route keys, in registration order.
   */
  async listProviders(): Promise<string[]> {
    const result = (await this.request('provider.list', {})) as {
      providers?: readonly unknown[]
    }
    const ids = result.providers ?? []
    return ids.filter((id): id is string => typeof id === 'string')
  }

  /** The endpoint-reported model catalog for one registered provider id.
   * @param provider - the route key to query.
   * @returns raw catalog rows; shape is owned by the sidecar's SDK.
   */
  async listModels(provider: string): Promise<unknown[]> {
    const rows: unknown = await this.request('model.list', { provider })
    if (!Array.isArray(rows)) return []
    const out: unknown[] = []
    for (const row of rows) out.push(row)
    return out
  }

  /**
   * Start one streamed completion. The returned iterable yields events until
   * the sidecar's terminal notification; breaking out sends `stream.cancel`
   * so the Rust-side HTTP request aborts promptly. A stalled read settles
   * when `readSignal` aborts (idle watchdog or caller), surfacing a typed
   * timeout or cancelled failure instead of waiting on the child forever.
   */
  /**
   * Start one streamed completion over the live transport.
   * @param reference - route:model selector the child resolves.
   * @param request - the assembled wire request (see {@link WireChatRequest}).
   * @param readSignal - aborts settle an outstanding read as cancelled/timeout.
   * @returns an async iterable of events terminated by chat/done.
   */
  async * stream(reference: string, request: WireChatRequest, readSignal?: AbortSignal): SidecarStream {
    const transport = await this.start()
    const streamId = randomUUID()
    const state: StreamState = { buffer: [], finished: false, failure: undefined, wake: undefined }
    const onReadAbort = (): void => {
      if (state.finished) return
      state.finished = true
      state.failure ??= new SidecarProtocolError(
        readSignal !== undefined && timeoutOf(readSignal, STREAM_IDLE_TIMEOUT_CODE) !== undefined
          ? 'timeout'
          : 'cancelled',
        'llm-ai-sdk: ai-sidecar stream read did not settle',
        true,
      )
      state.wake?.()
      state.wake = undefined
    }
    if (readSignal === undefined || readSignal.aborted) onReadAbort()
    else readSignal.addEventListener('abort', onReadAbort, { once: true })
    this.streams.set(streamId, state)
    try {
      await Promise.race([
        transport.request('chat.stream', { stream_id: streamId, reference, request }),
        this.childExitOrTimeout(0),
      ])
    } catch (error: unknown) {
      this.streams.delete(streamId)
      if (error instanceof JsonRpcResponseError) {
        const data = (error.data ?? {}) as { kind?: string; retryable?: boolean }
        throw new SidecarProtocolError(data.kind, error.message, data.retryable, error)
      }
      throw error
    }
    try {
      while (!state.finished || state.buffer.length > 0) {
        if (state.buffer.length > 0) {
          const next = state.buffer.shift()
          if (next === undefined) break
          yield next
          continue
        }
        if (state.failure !== undefined) throw state.failure
        await new Promise<void>((resolve) => { state.wake = resolve })
      }
      if (state.failure !== undefined) throw state.failure
    } finally {
      readSignal?.removeEventListener('abort', onReadAbort)
      this.streams.delete(streamId)
      if (!state.finished) {
        transport.notify('stream.cancel', { stream_id: streamId })
      }
    }
  }

  /** Drain deadline for a quiesce: let in-flight pumps reach chat/done before failing them. */
  async drain(deadlineMs: number): Promise<void> {
    if (this.streams.size === 0) return
    const deadline = Date.now() + deadlineMs
    while (this.streams.size > 0 && Date.now() < deadline) {
      await new Promise<void>(resolve => setTimeout(resolve, 50))
    }
    if (this.streams.size > 0) {
      this.failAllStreams(
        new SidecarProtocolError('network', 'llm-ai-sdk: sidecar drain deadline exceeded', true),
      )
    }
  }

  /**
   * Spawn a fresh shadow client probing the same binary. Callers must
   * `configure()` the shadow with the current generation before promoting.
   * @param connection - connection facts for the shadow (may be same binary).
   * @returns a new client that has completed initialize.
   */
  async spawnShadow(connection: SidecarConnection): Promise<AiSidecarClient> {
    const shadow = new AiSidecarClient(() => connection)
    await (shadow as unknown as { start: () => Promise<unknown> }).start()
    return shadow
  }

  /**
   * Health probe: list providers on this client.
   * @returns provider ids; throws on failure.
   */
  async healthProbe(): Promise<string[]> {
    return await this.listProviders()
  }

  /**
   * Atomically promote a healthy shadow by stealing its transport. The shadow
   * is disposed after promotion; the old generation drains to its deadline.
   * @param shadow - a healthy shadow that has been configured.
   * @param drainDeadlineMs - wall time to let old pumps reach chat/done.
   */
  async promoteShadow(shadow: AiSidecarClient, drainDeadlineMs = 5000): Promise<void> {
    if (shadow === this) throw new Error('llm-ai-sdk: cannot promote self as shadow')
    const shadowChild = (shadow as unknown as { child: ReturnType<typeof import('node:child_process').spawn> | undefined }).child
    const shadowTransport = (shadow as unknown as { transport: JsonRpcLineTransport | undefined }).transport
    const shadowInitialized = (shadow as unknown as { initialized: Promise<void> | undefined }).initialized
    ;(shadow as unknown as { child: unknown }).child = undefined
    ;(shadow as unknown as { transport: unknown }).transport = undefined
    ;(shadow as unknown as { initialized: unknown }).initialized = undefined
    ;(shadow as unknown as { disposed: boolean }).disposed = true
    const oldChild = this.child
    const oldTransport = this.transport
    this.child = shadowChild
    this.transport = shadowTransport
    this.initialized = shadowInitialized
    if (oldChild !== undefined && oldChild !== shadowChild) {
      try { oldTransport?.close() } catch {}
      await new Promise<void>(resolve => setTimeout(resolve, Math.min(drainDeadlineMs, 200)))
      try { oldChild.kill() } catch {}
    }
  }

  private failAllStreams(error: Error): void {
    for (const state of this.streams.values()) {
      state.finished = true
      state.failure ??= error
      state.wake?.()
      state.wake = undefined
    }
  }

  /** Kill the child and reject everything still outstanding. Idempotent. */
  dispose(): void {
    this.disposed = true
    for (const timer of this.pendingTimeouts.values()) clearTimeout(timer)
    this.pendingTimeouts.clear()
    // The disposal wording wins over the generic exit wording: streams still
    // outstanding when the client is torn down report why, not how.
    this.failAllStreams(new SidecarProtocolError(undefined, 'llm-ai-sdk: sidecar disposed'))
    this.onChildGone(undefined)
  }
}
