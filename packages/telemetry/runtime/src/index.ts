/**
 * Process-wide tracing and metrics for the harness. One `ctx.telemetry`
 * service owns bounded span and event rings plus per-Session and per-route
 * rollups, so the only cost on a hot path is an array write.
 *
 * The rings are exposed two ways: over `GET /api/diagnostics` (JSON),
 * `GET /api/diagnostics/stream` (SSE), and `POST /api/diagnostics/client`
 * (browser spans), and to other Host plugins through {@link TelemetryRuntime.snapshot}.
 * Nothing here writes a log file or calls `console`.
 * @module @deepseek-ai/dsh-telemetry-runtime
 */

import { performance } from 'node:perf_hooks'
import type { IncomingMessage, ServerResponse } from 'node:http'
import { Context, Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type { GenerateOptions, StreamChunk } from '@deepseek-ai/dsh-llm'
import type {} from '@deepseek-ai/dsh-session'
import type {} from '@deepseek-ai/dsh-tools'
import type {} from '@deepseek-ai/dsh-host-webserver'
// The `ctx.loader` Context merge, whose rows the snapshot projects.
import type {} from '@deepseek-ai/cordis-plugin-loader'
import { Ring } from './rings.ts'
import type {
  ClientSpan, CrashRecord, LatencySummary, LoaderRow, TelemetryEvent, TelemetrySnapshot,
} from './types.ts'

export type * from './types.ts'
export { Ring } from './rings.ts'

declare module '@deepseek-ai/cordis' {
  interface Context {
    /** Process-wide tracing and metrics. */
    telemetry: TelemetryRuntime
  }
}

/** Plugin name. */
export const name = 'telemetry-runtime'

/** The Loader, whose rows the snapshot projects; every other seam is optional. */
export const inject = ['loader'] as const

/** Deployment policy of the tracing rings. */
export interface Config {
  /** Retained span and event entries (default 2000). */
  readonly events?: number
  /** Retained browser spans (default 500). */
  readonly clientSpans?: number
  /** Retained crash records (default 100). */
  readonly crashes?: number
  /** Duration above which a span is also recorded as a slow event (default 500 ms). */
  readonly slowMs?: number
  /** Largest accepted browser report body (default 65536 bytes). */
  readonly maxBodyBytes?: number
  /** Serve the diagnostics HTTP endpoints (default true). */
  readonly serveDiagnostics?: boolean
}

/** Retained duration samples per span name; older samples are dropped. */
const SAMPLE_LIMIT = 512

/** Bounded per-name duration accumulator. */
interface LatencyBucket {
  durations: number[]
  errors: number
}

/** Mutable per-Session rollup. */
interface SessionBucket {
  sessionId: string
  turns: number
  steps: number
  toolCalls: number
  inputTokens: number
  outputTokens: number
  lastAt: string
  provider?: string | undefined
  model?: string | undefined
  turnStartedAt: number | undefined
  openTurn: number | undefined
}

/** Mutable per-route rollup. */
interface ModelBucket {
  provider: string
  model: string
  calls: number
  errors: number
  ttft: number[]
  durations: number[]
  lastAt: string
}

/**
 * The tracing service. Every method is synchronous and bounded: a caller may
 * record a span from any hot path without awaiting anything.
 */
export class TelemetryRuntime extends Service {
  static Config: z<Config> = z.object({
    events: z.natural().min(1).default(2000),
    clientSpans: z.natural().min(1).default(500),
    crashes: z.natural().min(1).default(100),
    slowMs: z.natural().min(1).default(500),
    maxBodyBytes: z.natural().min(1).default(65536),
    serveDiagnostics: z.boolean().default(true),
  })

  private readonly events: Ring<TelemetryEvent>
  private readonly clientSpans: Ring<ClientSpan>
  private readonly crashRecords: Ring<CrashRecord>
  private readonly counters = new Map<string, number>()
  private readonly latencies = new Map<string, LatencyBucket>()
  private readonly sessions = new Map<string, SessionBucket>()
  private readonly models = new Map<string, ModelBucket>()
  private readonly listeners = new Set<(event: TelemetryEvent) => void>()
  private readonly slowMs: number
  private readonly maxBodyBytes: number
  private seq = 0
  private readonly startedAt = Date.now()

  /**
   * @param ctx - owning context, whose Loader rows the snapshot projects.
   * @param config - ring sizes, slow-span threshold, and endpoint policy.
   */
  constructor(ctx: Context, config: Config) {
    super(ctx, 'telemetry')
    const resolved = config as Required<Config>
    this.events = new Ring<TelemetryEvent>(resolved.events)
    this.clientSpans = new Ring<ClientSpan>(resolved.clientSpans)
    this.crashRecords = new Ring<CrashRecord>(resolved.crashes)
    this.slowMs = resolved.slowMs
    this.maxBodyBytes = resolved.maxBodyBytes
    this.instrument()
    if (resolved.serveDiagnostics) {
      ctx.inject(['webServer'], (web) => {
        web.effect(() => web.webServer.register({
          kind: 'prefix', path: '/api/diagnostics', handler: (req, res) => this.handle(req, res),
        }), 'telemetry: diagnostics endpoint')
      })
    }
  }

  /**
   * Record one counter increment.
   * @param name - counter name, for example `tool.calls`.
   * @param delta - amount to add, default 1.
   */
  count(name: string, delta: number = 1): void {
    this.counters.set(name, (this.counters.get(name) ?? 0) + delta)
  }

  /**
   * Record one completed span.
   * @param kind - category (`tool`, `model`, `turn`, `plugin`, `client`).
   * @param spanName - span name.
   * @param durationMs - measured duration in milliseconds.
   * @param fields - optional route, Session, turn, and outcome facts.
   */
  record(kind: string, spanName: string, durationMs: number, fields: {
    readonly ok?: boolean
    readonly sessionId?: string
    readonly turn?: number
    readonly provider?: string
    readonly model?: string
    readonly detail?: string
    readonly ttftMs?: number
  } = {}): void {
    const bucket = this.latencies.get(spanName) ?? { durations: [], errors: 0 }
    bucket.durations.push(durationMs)
    if (bucket.durations.length > SAMPLE_LIMIT) bucket.durations.shift()
    if (fields.ok === false) bucket.errors += 1
    this.latencies.set(spanName, bucket)
    this.emit({
      seq: ++this.seq,
      at: new Date().toISOString(),
      kind,
      name: spanName,
      durationMs: Math.round(durationMs * 10) / 10,
      ...fields.ok === undefined ? {} : { ok: fields.ok },
      ...fields.sessionId === undefined ? {} : { sessionId: fields.sessionId },
      ...fields.turn === undefined ? {} : { turn: fields.turn },
      ...fields.provider === undefined ? {} : { provider: fields.provider },
      ...fields.model === undefined ? {} : { model: fields.model },
      ...fields.detail === undefined ? {} : { detail: fields.detail },
      ...fields.ttftMs === undefined ? {} : { ttftMs: Math.round(fields.ttftMs * 10) / 10 },
    })
    if (durationMs >= this.slowMs) this.count('span.slow')
  }

  /**
   * Record one crash with the cause's first frames.
   * @param scope - where it was caught (`plugin`, `model`, `tool`, `client`).
   * @param error - the caught value.
   */
  crash(scope: string, error: unknown): void {
    const message = firstLine(error)
    const stack = error instanceof Error && error.stack !== undefined
      ? error.stack.split('\n').slice(0, 6).join('\n')
      : undefined
    const record: CrashRecord = { at: new Date().toISOString(), scope, message, ...stack === undefined ? {} : { stack } }
    this.crashRecords.push(record)
    this.count(`crash.${scope}`)
    this.emit({ seq: ++this.seq, at: record.at, kind: 'crash', name: scope, ok: false, detail: message })
  }

  /**
   * Ingest one browser span.
   * @param span - the reported span.
   */
  client(span: ClientSpan): void {
    this.clientSpans.push(span)
    this.count(`client.${span.kind}`)
    this.emit({
      seq: ++this.seq, at: span.at, kind: 'client', name: span.name, durationMs: span.durationMs,
      ...span.ok === undefined ? {} : { ok: span.ok },
      ...span.detail === undefined ? {} : { detail: span.detail },
    })
  }

  /**
   * Observe every new span as the rings accept it.
   * @param listener - invoked synchronously per record.
   * @returns disposer removing the listener.
   */
  subscribe(listener: (event: TelemetryEvent) => void): () => void {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
  }

  /**
   * Read the complete diagnostics snapshot.
   * @param eventLimit - newest events included, bounded by the ring.
   * @returns the snapshot the HTTP endpoint serves.
   */
  snapshot(eventLimit: number = 200): TelemetrySnapshot {
    const events = this.events.toArray()
    const crashes = this.crashRecords.toArray()
    const loader = this.loaderRows()
    const failed = loader.filter(row => row.state === 'FAILED')
    const reasons: string[] = []
    for (const row of failed) reasons.push(`plugin ${row.id} failed${row.error === undefined ? '' : `: ${row.error}`}`)
    const lastCrash = crashes.at(-1)
    if (lastCrash !== undefined && Date.now() - Date.parse(lastCrash.at) < 60_000) {
      reasons.push(`recent ${lastCrash.scope} crash: ${lastCrash.message}`)
    }
    const memory = process.memoryUsage()
    return {
      generatedAt: new Date().toISOString(),
      process: {
        pid: process.pid,
        node: process.version,
        platform: process.platform,
        arch: process.arch,
        uptimeMs: Date.now() - this.startedAt,
        rss: memory.rss,
        heapUsed: memory.heapUsed,
        heapTotal: memory.heapTotal,
      },
      counters: Object.fromEntries([...this.counters.entries()].sort((left, right) => left[0].localeCompare(right[0]))),
      latency: this.latencySummaries(),
      sessions: [...this.sessions.values()].map(session => ({
        sessionId: session.sessionId,
        turns: session.turns,
        steps: session.steps,
        toolCalls: session.toolCalls,
        inputTokens: session.inputTokens,
        outputTokens: session.outputTokens,
        lastAt: session.lastAt,
        ...session.provider === undefined ? {} : { provider: session.provider },
        ...session.model === undefined ? {} : { model: session.model },
      })).sort((left, right) => right.lastAt.localeCompare(left.lastAt)),
      models: [...this.models.values()].map(model => ({
        provider: model.provider,
        model: model.model,
        calls: model.calls,
        errors: model.errors,
        ttftP50: percentile(model.ttft, 50),
        ttftP95: percentile(model.ttft, 95),
        durationP50: percentile(model.durations, 50),
        lastAt: model.lastAt,
      })).sort((left, right) => right.lastAt.localeCompare(left.lastAt)),
      loader,
      crashes,
      events: events.slice(Math.max(0, events.length - eventLimit)),
      client: this.clientSpans.toArray(),
      health: { ok: reasons.length === 0, reasons },
    }
  }

  /** Subscribe to the tracing seams this plugin instruments. */
  private instrument(): void {
    const ctx = this.ctx
    ctx.on('llm/stream', (options, next) => this.instrumentModel(options, next))
    ctx.on('tools/execute', async (exec, next) => {
      const start = performance.now()
      const sessionId = exec.agent?.session.id
      try {
        const result = await next()
        this.record('tool', `tool:${exec.name}`, performance.now() - start, {
          ok: true, ...sessionId === undefined ? {} : { sessionId: String(sessionId) },
        })
        return result
      } catch (error) {
        this.record('tool', `tool:${exec.name}`, performance.now() - start, {
          ok: false, detail: firstLine(error), ...sessionId === undefined ? {} : { sessionId: String(sessionId) },
        })
        this.crash('tool', error)
        throw error
      }
    })
    ctx.on('session/event', (session, event) => { this.sessionEvent(String(session.id), event) })
  }

  /** Wrap one model stream: time to first chunk, total duration, tokens, and failure. */
  private instrumentModel(
    options: GenerateOptions,
    next: () => AsyncIterable<StreamChunk>,
  ): AsyncIterable<StreamChunk> {
    const start = performance.now()
    const sessionId = options.sessionId === undefined ? undefined : String(options.sessionId)
    let ttft: number | undefined
    const self = this
    return (async function* () {
      let inputTokens = 0
      let outputTokens = 0
      try {
        for await (const chunk of next()) {
          if (ttft === undefined) ttft = performance.now() - start
          if (chunk.type === 'usage') {
            inputTokens += chunk.usage.inputTokens
            outputTokens += chunk.usage.outputTokens
          }
          yield chunk
        }
      } catch (error) {
        self.finishModel(options, start, ttft, false, sessionId, inputTokens, outputTokens, firstLine(error))
        self.crash('model', error)
        throw error
      }
      self.finishModel(options, start, ttft, true, sessionId, inputTokens, outputTokens)
    })()
  }

  /** Fold one completed model call into its route and its Session. */
  private finishModel(
    options: { provider: string; model: string },
    start: number,
    ttft: number | undefined,
    ok: boolean,
    sessionId: string | undefined,
    inputTokens: number,
    outputTokens: number,
    detail?: string,
  ): void {
    const duration = performance.now() - start
    const key = `${options.provider}/${options.model}`
    const bucket = this.models.get(key) ?? {
      provider: options.provider, model: options.model, calls: 0, errors: 0, ttft: [], durations: [], lastAt: '',
    }
    bucket.calls += 1
    if (!ok) bucket.errors += 1
    if (ttft !== undefined) {
      bucket.ttft.push(ttft)
      if (bucket.ttft.length > SAMPLE_LIMIT) bucket.ttft.shift()
    }
    bucket.durations.push(duration)
    if (bucket.durations.length > SAMPLE_LIMIT) bucket.durations.shift()
    bucket.lastAt = new Date().toISOString()
    this.models.set(key, bucket)
    this.count('model.calls')
    if (!ok) this.count('model.errors')
    if (sessionId !== undefined) {
      const session = this.session(sessionId)
      session.provider = options.provider
      session.model = options.model
      session.inputTokens += inputTokens
      session.outputTokens += outputTokens
      session.lastAt = bucket.lastAt
    }
    this.record('model', key, duration, {
      ok, ...ttft === undefined ? {} : { ttftMs: ttft },
      provider: options.provider, model: options.model,
      ...sessionId === undefined ? {} : { sessionId },
      ...detail === undefined ? {} : { detail },
    })
  }

  /** Fold one Session event into its rollup; turn latency is measured start to end. */
  private sessionEvent(sessionId: string, event: { type: string; data?: unknown }): void {
    const session = this.session(sessionId)
    session.lastAt = new Date().toISOString()
    switch (event.type) {
      case 'turn/start': {
        session.turns += 1
        session.turnStartedAt = performance.now()
        session.openTurn = (event.data as { turn?: number } | undefined)?.turn
        this.count('turn.started')
        return
      }
      case 'turn/end': {
        this.count('turn.completed')
        if (session.turnStartedAt === undefined) return
        this.record('turn', 'turn', performance.now() - session.turnStartedAt, {
          ok: true, sessionId, ...session.openTurn === undefined ? {} : { turn: session.openTurn },
        })
        session.turnStartedAt = undefined
        session.openTurn = undefined
        return
      }
      case 'step/start':
        session.steps += 1
        this.count('step.started')
        return
      case 'tool/call':
        session.toolCalls += 1
        return
      default:
        return
    }
  }

  /** Resolve or create one Session rollup. */
  private session(sessionId: string): SessionBucket {
    const existing = this.sessions.get(sessionId)
    if (existing !== undefined) return existing
    const created: SessionBucket = {
      sessionId, turns: 0, steps: 0, toolCalls: 0, inputTokens: 0, outputTokens: 0,
      lastAt: new Date().toISOString(), turnStartedAt: undefined, openTurn: undefined,
    }
    this.sessions.set(sessionId, created)
    return created
  }

  /** Project the Loader's live rows and their fiber states. */
  private loaderRows(): LoaderRow[] {
    return [...this.ctx.loader.entries()].map(entry => {
      const fiber = entry.fiber as { state?: number; error?: unknown } | undefined
      return {
        id: String(entry.options.id ?? entry.options.name),
        name: entry.options.name,
        state: entry.disabled ? 'DISABLED' : fiber === undefined ? 'UNLOADED' : fiberStateName(fiber.state),
        ...fiber?.error === undefined ? {} : { error: firstLine(fiber.error) },
      }
    })
  }

  /** Project each span name's p50/p95 and failure count. */
  private latencySummaries(): LatencySummary[] {
    return [...this.latencies.entries()]
      .map(([spanName, bucket]) => ({
        name: spanName,
        count: bucket.durations.length,
        errors: bucket.errors,
        p50: percentile(bucket.durations, 50),
        p95: percentile(bucket.durations, 95),
        max: bucket.durations.length === 0 ? 0 : Math.max(...bucket.durations),
      }))
      .sort((left, right) => right.p95 - left.p95)
  }

  /** Publish one event to the ring and its subscribers; a throwing subscriber is contained. */
  private emit(event: TelemetryEvent): void {
    this.events.push(event)
    for (const listener of [...this.listeners]) {
      try {
        listener(event)
      } catch (error) {
        this.counters.set('telemetry.subscriber-errors', (this.counters.get('telemetry.subscriber-errors') ?? 0) + 1)
        void error
      }
    }
  }

  /** Serve the JSON, SSE, and browser-ingest routes under `/api/diagnostics`. */
  private async handle(req: IncomingMessage, res: ServerResponse): Promise<void> {
    const url = new URL(req.url ?? '/', 'http://localhost')
    if (!this.trusted(req)) {
      writeJson(res, 403, { error: 'diagnostics: this connection is not a trusted local caller' })
      return
    }
    if (url.pathname === '/api/diagnostics' && req.method === 'GET') {
      const limit = Number(url.searchParams.get('events') ?? '200')
      writeJson(res, 200, this.snapshot(Number.isFinite(limit) ? Math.max(0, Math.trunc(limit)) : 200))
      return
    }
    if (url.pathname === '/api/diagnostics/stream' && req.method === 'GET') {
      res.writeHead(200, {
        'content-type': 'text/event-stream',
        'cache-control': 'no-store',
        connection: 'keep-alive',
      })
      res.write(`event: snapshot\ndata: ${JSON.stringify(this.snapshot(50))}\n\n`)
      const off = this.subscribe((event) => { res.write(`event: span\ndata: ${JSON.stringify(event)}\n\n`) })
      req.on('close', off)
      return
    }
    if (url.pathname === '/api/diagnostics/client' && req.method === 'POST') {
      const body = await readJson(req, this.maxBodyBytes)
      if (!Array.isArray(body)) {
        writeJson(res, 400, { error: 'diagnostics: the body must be a JSON array of spans' })
        return
      }
      let accepted = 0
      for (const candidate of body) {
        const span = clientSpanOf(candidate)
        if (span === undefined) continue
        this.client(span)
        accepted += 1
      }
      writeJson(res, 202, { accepted })
      return
    }
    writeJson(res, 404, { error: 'diagnostics: unknown route' })
  }

  /** Whether a request reached this process from the local machine. */
  private trusted(req: IncomingMessage): boolean {
    const remote = req.socket.remoteAddress ?? ''
    if (remote === '127.0.0.1' || remote === '::1' || remote === '::ffff:127.0.0.1') return true
    const host = (req.headers.host ?? '').replace(/:\d+$/u, '').replace(/^\[|\]$/gu, '')
    return host === '127.0.0.1' || host === 'localhost' || host === '::1'
  }
}

/** Map a Loader fiber state number to its name without importing the framework enum. */
function fiberStateName(state: number | undefined): string {
  switch (state) {
    case 0: return 'PENDING'
    case 1: return 'LOADING'
    case 2: return 'ACTIVE'
    case 3: return 'FAILED'
    case 4: return 'DISPOSED'
    case 5: return 'UNLOADING'
    default: return 'UNKNOWN'
  }
}

/** Nearest-rank percentile over a bounded sample, rounded to 0.1 ms. */
function percentile(samples: readonly number[], rank: number): number {
  if (samples.length === 0) return 0
  const sorted = [...samples].sort((left, right) => left - right)
  const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil((rank / 100) * sorted.length) - 1))
  return Math.round((sorted[index] as number) * 10) / 10
}

/** First line of an unknown error, bounded for the ring. */
function firstLine(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error)
  return message.split('\n')[0]?.slice(0, 300) ?? ''
}

/** Validate one browser-reported span. */
function clientSpanOf(value: unknown): ClientSpan | undefined {
  if (typeof value !== 'object' || value === null) return undefined
  const span = value as Record<string, unknown>
  if (typeof span['kind'] !== 'string' || typeof span['name'] !== 'string') return undefined
  if (typeof span['durationMs'] !== 'number' || !Number.isFinite(span['durationMs'])) return undefined
  return {
    at: typeof span['at'] === 'string' ? span['at'] : new Date().toISOString(),
    kind: span['kind'].slice(0, 32),
    name: span['name'].slice(0, 120),
    durationMs: Math.max(0, span['durationMs']),
    ...typeof span['ok'] === 'boolean' ? { ok: span['ok'] } : {},
    ...typeof span['detail'] === 'string' ? { detail: span['detail'].slice(0, 300) } : {},
  }
}

/** Write one JSON response. */
function writeJson(res: ServerResponse, status: number, body: unknown): void {
  res.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' })
  res.end(JSON.stringify(body))
}

/** Read a bounded JSON body; a malformed or oversized body rejects. */
async function readJson(req: IncomingMessage, maxBytes: number): Promise<unknown> {
  const chunks: Buffer[] = []
  let size = 0
  for await (const chunk of req) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk as string)
    size += buffer.length
    if (size > maxBytes) throw new Error('diagnostics: request body too large')
    chunks.push(buffer)
  }
  if (chunks.length === 0) return undefined
  return JSON.parse(Buffer.concat(chunks).toString('utf8')) as unknown
}

export default TelemetryRuntime
