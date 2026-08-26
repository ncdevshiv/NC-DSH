/**
 * `AiSdkAdapter`: one adapter instance serving every configured provider
 * route through the `ai-sidecar` child process. The adapter is
 * transport-only: routes arrive through a thunk resolved once per operation,
 * the bearer token through a per-request resolver, and the sidecar is
 * re-configured whenever the resolved credential/endpoint generation changes.
 * @module @deepseek-ai/dsh-llm-ai-sdk/adapter
 */

import { contentHasImage, LlmAdapter, LlmError, ReasoningEffortId } from '@deepseek-ai/dsh-llm'
import type {
  ContentBlock,
  GenerateOptions,
  LlmFailure,
  LlmModelInfo,
  LlmProviderInfo,
  LlmResolvedModelInfo,
  ResolvedRetryPolicy,
  StreamChunk,
} from '@deepseek-ai/dsh-llm'
import type { AttachmentStore, ImageAttachmentRef } from '@deepseek-ai/dsh-attachment'
import { idleWatchdog, timeoutOf } from '@deepseek-ai/dsh-timeout'
import type { RouteApiKind } from './types.ts'
import type { AiSidecarClient } from './sidecar.ts'
import { SidecarProtocolError } from './sidecar.ts'
import { finishReasonOf, StreamAssembler, toChatRequest } from './translate.ts'
import { STREAM_IDLE_TIMEOUT_CODE } from './sidecar.ts'
import type { ResolvedImages } from './translate.ts'

/** One advisory catalog model of a resolved route. */
export interface AiSdkCatalogModel {
  /** Model id the sidecar route accepts. */
  id: string
  /** Human-readable name for selectors when the composition supplies one. */
  name?: string
  /** Optional user-facing distinction from otherwise similar models. */
  description?: string
  /** Maximum combined request and response context, when declared. */
  contextWindow?: number
  /** Maximum output tokens, when declared. */
  maxTokens?: number
  /** Accepted request modalities; absent means unknown, never negative capability. */
  inputModalities?: readonly ('text' | 'image')[]
}

/** One fully-resolved provider route. */
export interface ResolvedRoute {
  /** Provider route key (`options.provider` selects by this). */
  id: string
  /** Selector label; defaults to {@link id}. */
  displayName: string
  /** Credential reference resolved per request. */
  apiKeyEnv: string
  /** Endpoint base; required for OpenAI-compatible ids without an SDK default. */
  baseURL: string | undefined
  /** Declared wire dialect; omission lets the sidecar derive from the route id. */
  api: RouteApiKind | undefined
  /** Advisory models exposed to discovery consumers. */
  models: readonly AiSdkCatalogModel[]
  /** Default per-request output cap; explicit request values win. */
  maxTokens: number
  /** Positive context capacity used when the selected model has no exact value. */
  defaultContextWindow: number
  /** Reasoning efforts this route exposes; empty means no effort metadata. */
  reasoningEfforts: readonly ('off' | 'low' | 'high' | 'max')[]
  /** Effort materialized into this route's requests when callers omit one. */
  defaultReasoningEffort: 'off' | 'low' | 'high' | 'max'
  /** Maximum accumulated base64 image payload in one request on this route. */
  maxRequestImageBytes: number
}

/** All resolved routes plus the connection facts of one resolution generation. */
export interface ResolvedRouteSet {
  /** Absolute `ai-sidecar` executable path shared by every route. */
  binaryPath: string
  routes: ReadonlyMap<string, ResolvedRoute>
}

/** Constructor hooks the owning plugin supplies. */
export interface AiSdkAdapterOptions {
  /** Current validated route facts; called once per operation. */
  options: () => ResolvedRouteSet
  /**
   * Resolve the bearer key for one route's own resolution snapshot. Throws
   * `LlmError` `MISSING_CREDENTIAL` when no key is available anywhere.
   */
  resolveApiKey: (route: ResolvedRoute) => Promise<string>
  /** Resolve the current durable attachment service; absence rejects image input. */
  resolveAttachments: () => AttachmentStore | undefined
}

/** Default maximum idle interval while a stream read is outstanding. */
export const DEFAULT_STREAM_IDLE_TIMEOUT_MS = 300_000

/** Map a sidecar typed error kind onto the harness failure code vocabulary. */
function errorCodeOf(kind: string | undefined): string {
  switch (kind) {
    case 'rate_limit': return 'RATE_LIMIT'
    case 'authentication': return 'INVALID_CREDENTIAL'
    case 'configuration': return 'CONFIG'
    case 'timeout': return 'TIMEOUT'
    case 'cancelled': return 'ABORTED'
    case 'network':
    case 'serialization': return 'TRANSPORT'
    default: return 'PROVIDER'
  }
}

function toLlmError(error: unknown, label: string): LlmError {
  if (error instanceof LlmError) return error
  if (error instanceof SidecarProtocolError) {
    const failure: LlmFailure = { message: error.message, code: errorCodeOf(error.kind) }
    throw new LlmError(`${label}: ${error.message}`, failure.code, { cause: error })
  }
  throw new LlmError(`${label}: ${String(error)}`, 'TRANSPORT', { cause: error })
}

/**
 * Resolve image attachments to inline base64 parts before request assembly.
 * @returns attachment-id keyed bytes; empty for text-only requests.
 */
async function resolveImages(
  options: GenerateOptions,
  attachments: AttachmentStore | undefined,
  maxRequestImageBytes: number,
): Promise<ResolvedImages> {
  const refs = new Map<string, { mediaType: string; bytes: Uint8Array }>()
  const collect = (blocks: readonly ContentBlock[]): void => {
    for (const block of blocks) {
      if (block.type === 'image') {
        refs.set(block.attachment.attachmentId, {
          mediaType: block.attachment.mediaType,
          bytes: new Uint8Array(0),
        })
      } else if (block.type === 'tool-result') {
        collect(block.content)
      }
    }
  }
  for (const message of options.messages) collect(message.content)
  if (refs.size === 0) return new Map()
  if (attachments === undefined) {
    throw new LlmError('AI SDK image conversion requires the durable attachment service.', 'UNSUPPORTED_CONTENT')
  }
  let accumulated = 0
  const resolved: [string, { mediaType: string; base64: string }][] = []
  for (const [id] of refs) {
    const stored = await attachments.readImage(
      findRef(options.messages, id),
      options.signal,
    )
    accumulated += stored.data.length
    if (accumulated > maxRequestImageBytes) {
      throw new LlmError(
        `AI SDK request image payload exceeds the ${maxRequestImageBytes}-byte route cap`,
        'UNSUPPORTED_CONTENT',
      )
    }
    let binary = ''
    const chunkSize = 0x8000
    for (let offset = 0; offset < stored.data.length; offset += chunkSize) {
      binary += String.fromCharCode(...stored.data.subarray(offset, offset + chunkSize))
    }
    resolved.push([id, { mediaType: stored.ref.mediaType, base64: btoa(binary) }])
  }
  return new Map(resolved)
}

/** Locate one message's image reference by attachment id (pre-validated by {@link resolveImages}). */
function findRef(
  messages: GenerateOptions['messages'],
  attachmentId: string,
): ImageAttachmentRef {
  for (const message of messages) {
    for (const block of message.content) {
      if (block.type === 'image' && block.attachment.attachmentId === attachmentId) return block.attachment
      if (block.type === 'tool-result') {
        const nested = block.content.find(candidate =>
          candidate.type === 'image' && candidate.attachment.attachmentId === attachmentId)
        if (nested?.type === 'image') return nested.attachment
      }
    }
  }
  throw new LlmError(`AI SDK adapter lost track of image attachment "${attachmentId}"`, 'INTERNAL')
}

/**
 * The only adapter. Every provider route — DeepSeek official, OpenRouter,
 * Ollama, Anthropic, Gemini, any gateway — resolves through this instance;
 * the sidecar owns each wire protocol.
 */


/** Map one caught error onto the {@link LlmFailure} vocabulary carried in `finish`. */
function toFailure(error: unknown): LlmFailure {
  if (error instanceof SidecarProtocolError) {
    return { message: error.message, code: errorCodeOf(error.kind) }
  }
  if (error instanceof LlmError) {
    return { message: error.message, code: error.code }
  }
  return { message: error instanceof Error ? error.message : String(error), code: 'TRANSPORT' }
}
/**
 * The one adapter. Every provider route — DeepSeek official, OpenRouter,
 * Ollama, Anthropic, Gemini, any gateway — resolves through this instance;
 * the sidecar owns each wire protocol.
 */
export class AiSdkAdapter extends LlmAdapter {
  private configuredGeneration: string | undefined

  constructor(
    private readonly config: AiSdkAdapterOptions,
    private readonly sidecar: AiSidecarClient,
    private readonly idleTimeoutMs: () => number,
    private readonly retryPolicyOf: (route: ResolvedRoute) => ResolvedRetryPolicy,
  ) {
    super()
  }

  override providerInfo(provider: string): LlmProviderInfo {
    const route = this.config.options().routes.get(provider)
    return { id: provider, name: route?.displayName ?? provider }
  }

  override providerRetryPolicy(provider: string): ResolvedRetryPolicy | undefined {
    const route = this.config.options().routes.get(provider)
    return route === undefined ? undefined : this.retryPolicyOf(route)
  }

  override listModels(provider: string): Promise<readonly LlmModelInfo[]> {
    const route = this.route(provider)
    return Promise.resolve(route.models.map(model => ({
      provider,
      id: model.id,
      name: model.name ?? model.id,
      ...(model.description === undefined ? {} : { description: model.description }),
      inputModalities: model.inputModalities ?? ['text'],
    })))
  }

  override resolveModel(
    provider: string,
    model: string,
    _signal?: AbortSignal,
  ): Promise<LlmResolvedModelInfo> {
    const route = this.route(provider)
    const configured = route.models.find(entry => entry.id === model)
    // An uncatalogued endpoint is safely treated as text-only; declaring an
    // unverified image capability would persist input the endpoint may reject.
    const efforts = route.reasoningEfforts.map(id => ({ id: ReasoningEffortId(id), name: id.toUpperCase() })) as ReadonlyArray<import('@deepseek-ai/dsh-llm').LlmReasoningEffortInfo>
    const info: LlmResolvedModelInfo = {
      ...configured === undefined
        ? { provider, id: model, name: model, inputModalities: ['text' as const] }
        : {
          provider,
          id: configured.id,
          name: configured.name ?? configured.id,
          ...(configured.description === undefined ? {} : { description: configured.description }),
          inputModalities: configured.inputModalities ?? ['text'],
        },
      context: { contextWindow: configured?.contextWindow ?? route.defaultContextWindow },
      defaultMaxTokens: configured?.maxTokens ?? route.maxTokens,
    }
    if (efforts.length > 0) {
      const reasoning: import('@deepseek-ai/dsh-llm').LlmModelReasoningInfo = { efforts }
      reasoning.defaultEffort = ReasoningEffortId(route.defaultReasoningEffort)
      info.reasoning = reasoning
    }
    return Promise.resolve(info)
  }

  async * stream(options: GenerateOptions): AsyncIterable<StreamChunk> {
    const resolved = this.config.options()
    const route = resolved.routes.get(options.provider)
    if (route === undefined) {
      throw new LlmError(`no AI SDK route "${options.provider}" is currently registered`, 'NO_ADAPTER')
    }
    const hasImages = options.messages.some(message => contentHasImage(message.content))
    if (hasImages) {
      const catalogued = route.models.find(entry => entry.id === options.model)
      if (catalogued?.inputModalities?.includes('image') !== true) {
        throw new LlmError(
          `AI SDK model "${options.model}" on route "${route.id}" does not accept image input.`,
          'UNSUPPORTED_CONTENT',
        )
      }
    }
    // The requested route's key is required; sibling routes join the configure
    // generation only when their credentials resolve without throwing.
    const apiKey = await this.config.resolveApiKey(route)
    await this.ensureConfigured(resolved, route, apiKey)

    // The route default fills an omitted effort so a deployment's thinking
    // posture holds without every caller naming it.
    const base: GenerateOptions = options
    const effective: GenerateOptions = options.reasoningEffort !== undefined
      ? base
      : { ...base, reasoningEffort: ReasoningEffortId(route.defaultReasoningEffort) }
    const images = hasImages
      ? await resolveImages(effective, this.config.resolveAttachments(), route.maxRequestImageBytes)
      : new Map<string, { mediaType: string; base64: string }>()
    const request = toChatRequest(effective, images)

    const consumer = new AbortController()
    const upstream = options.signal === undefined
      ? consumer.signal
      : AbortSignal.any([options.signal, consumer.signal])
    using watchdog = idleWatchdog(upstream, this.idleTimeoutMs(), STREAM_IDLE_TIMEOUT_CODE)
    const assembler = new StreamAssembler()
    const iterator = this.pump(options, route.id, request, assembler, watchdog.signal)[Symbol.asyncIterator]()
    let exhausted = false
    try {
      while (true) {
        const result = await watchdog.next(iterator)
        if (result.done) {
          exhausted = true
          return
        }
        yield result.value
      }
    } catch (error: unknown) {
      if (timeoutOf(watchdog.signal, STREAM_IDLE_TIMEOUT_CODE) !== undefined) {
        throw new LlmError('AI SDK stream idle timeout', 'TIMEOUT', { cause: error })
      }
      if (options.signal?.aborted) {
        throw new LlmError('AI SDK request aborted by caller', 'ABORTED', { cause: error })
      }
      throw toLlmError(error, 'llm-ai-sdk')
    } finally {
      consumer.abort('AI SDK stream consumer stopped')
      if (!exhausted && iterator.return !== undefined) {
        try {
          await iterator.return()
        } catch (_abortedTransportTeardown) {
          // The consumer controller already owns termination; a return-time
          // abort cannot add a second outcome.
        }
      }
    }
  }

  private route(provider: string): ResolvedRoute {
    const route = this.config.options().routes.get(provider)
    if (route === undefined) {
      throw new LlmError(`no AI SDK route "${provider}" is currently registered`, 'NO_ADAPTER')
    }
    return route
  }

  /**
   * Push the current credential/endpoint generation to the child exactly once
   * per change. In-flight streams keep their started generation because the
   * child replaces its client only between requests it has already accepted.
   */
  private async ensureConfigured(
    resolved: ResolvedRouteSet,
    required: ResolvedRoute,
    requiredKey: string,
  ): Promise<void> {
    const providers: Record<string, { api_key: string; base_url?: string; api?: RouteApiKind }> = {}
    const addRoute = (route: ResolvedRoute, key: string): void => {
      providers[route.id] = {
        api_key: key,
        ...(route.baseURL === undefined ? {} : { base_url: route.baseURL }),
        ...(route.api === undefined ? {} : { api: route.api }),
      }
    }
    addRoute(required, requiredKey)
    for (const route of resolved.routes.values()) {
      if (route.id === required.id || providers[route.id] !== undefined) continue
      try {
        addRoute(route, await this.config.resolveApiKey(route))
      } catch {
        // A sibling route without credentials stays unconfigured; requesting
        // it later re-runs this step and fails with its own MISSING_CREDENTIAL.
      }
    }
    const generation = JSON.stringify(providers)
    if (generation === this.configuredGeneration) return
    await this.sidecar.configure(providers)
    this.configuredGeneration = generation
  }

  private async * pump(
    options: GenerateOptions,
    _routeId: string,
    request: ReturnType<typeof toChatRequest>,
    assembler: StreamAssembler,
    readSignal: AbortSignal,
  ): AsyncIterable<StreamChunk> {
    const reference = `${options.provider}:${options.model}`
    let finishReason: string | undefined
    try {
      for await (const event of this.sidecar.stream(reference, request, readSignal)) {
        if (event.type === 'completed') finishReason = event.finish_reason ?? undefined
        for (const chunk of assembler.accept(event)) yield chunk
      }
      for (const chunk of assembler.close()) yield chunk
      yield { type: 'finish', reason: finishReasonOf(finishReason) }
    } catch (error: unknown) {
      if (
        error instanceof SidecarProtocolError
        && (error.kind === 'cancelled' || error.kind === 'timeout')
      ) {
        // Cancellation and idle expiry are transport outcomes, not provider
        // answers: they propagate as throws so the caller sees ABORTED or
        // TIMEOUT rather than an in-band failure finish.
        throw toLlmError(error, 'llm-ai-sdk')
      }
      // Thrown failures are normalized by LlmRuntime into a terminal
      // `error`/`aborted` finish; nothing else may follow the close.
      for (const chunk of assembler.close()) yield chunk
      const failure: LlmFailure = toFailure(error)
      yield { type: 'finish', reason: { kind: 'error', failure } }
      return
    }
  }
}
