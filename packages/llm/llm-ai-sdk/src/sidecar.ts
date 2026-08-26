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
      new SidecarProtocolError(undefined, 'llm-ai-sdk: ai-sidecar exited before completing its streams'),
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
  private async request(method: string, params: object): Promise<unknown> {
    const transport = await this.start()
    const id = ++this.nextRequestId
    try {
      return await Promise.race([
        transport.request(method, params),
        this.childExitOrTimeout(id),
      ])
    } catch (error: unknown) {
      if (error instanceof JsonRpcResponseError) {
        const data = (error.data ?? {}) as { kind?: string; retryable?: boolean }
        throw new SidecarProtocolError(data.kind, error.message, data.retryable, error)
      }
      throw error
    }
  }

  /** Timeout race partner; child exit or spawn failure rejects immediately. */
  private childExitOrTimeout(_id: number): Promise<never> {
    return new Promise((_resolve, reject) => {
      const timer = setTimeout(
        () => {
          reject(new SidecarProtocolError('timeout', 'llm-ai-sdk: sidecar request timed out', true))
        },
        DEFAULT_REQUEST_TIMEOUT_MS,
      )
      timer.unref()
      this.child?.once('exit', () => {
        reject(new SidecarProtocolError(undefined, 'llm-ai-sdk: ai-sidecar exited mid-request'))
      })
      // A failed spawn emits `error` without `exit`; without this the request
      // would hang until the ceiling instead of failing at launch.
      this.child?.once('error', () => {
        reject(new SidecarProtocolError(undefined, 'llm-ai-sdk: ai-sidecar failed to launch'))
      })
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
  ): Promise<SidecarDiscoveredModel[]> {
    const params: SidecarDiscoverParams = {
      ...(request.apiKey === undefined ? {} : { api_key: request.apiKey }),
      ...(request.baseURL === undefined ? {} : { base_url: request.baseURL }),
      ...(request.api === undefined ? {} : { api: request.api }),
    }
    const result = (await this.request('model.discover', params)) as {
      models?: readonly unknown[]
    }
    const rows = result.models ?? []
    const out: SidecarDiscoveredModel[] = []
    for (const row of rows) out.push(row as SidecarDiscoveredModel)
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
   * @param reference -oute:model selector the child resolves.
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
    // The disposal wording wins over the generic exit wording: streams still
    // outstanding when the client is torn down report why, not how.
    this.failAllStreams(new SidecarProtocolError(undefined, 'llm-ai-sdk: sidecar disposed'))
    this.onChildGone(undefined)
  }
}
