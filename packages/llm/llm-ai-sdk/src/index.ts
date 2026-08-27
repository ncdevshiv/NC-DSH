/**
 * Register the {@link AiSdkAdapter} for every configured provider route on
 * `ctx.llm`, backed by one long-lived `ai-sidecar` child process (the Rust
 * AI SDK's stdio JSON-RPC gateway). Connection facts are resolved per request
 * instead of frozen at load: the plugin layers its `cordis.yml` entry config
 * under the optional `llm-ai-sdk` user-settings section (`ctx.settings`) and
 * resolves each route's API key through the optional credential seam
 * (`ctx.credentials`), so a changed base URL, catalog, or key reaches the
 * very next request without restarting anything, while an in-flight stream
 * keeps the facts it started with. The sidecar is re-configured only when the
 * resolved credential/endpoint generation changes.
 *
 * Every provider route this plugin owns — DeepSeek official, OpenRouter,
 * Ollama, Anthropic, Gemini, any OpenAI-compatible gateway — resolves through
 * the single AI SDK adapter; no other package speaks a provider wire.
 * @module @deepseek-ai/dsh-llm-ai-sdk
 */

import type { Context } from '@deepseek-ai/cordis'
import { existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import z from '@deepseek-ai/schemastery'
import { assertUsableApiKey, LlmError, resolveRetryPolicy, RetryPolicySchema } from '@deepseek-ai/dsh-llm'
import type { LlmConfigurableProvider, LlmDiscoveredModel, ModelModality, RetryPolicyConfig } from '@deepseek-ai/dsh-llm'
import type { AttachmentStore } from '@deepseek-ai/dsh-attachment'
import { credentialRef } from '@deepseek-ai/dsh-credentials'
import type { CredentialRef } from '@deepseek-ai/dsh-credentials'
import { launchEnvironmentOf, type LaunchEnvironmentSnapshot } from '@deepseek-ai/dsh-launch-environment'
import { installSettingsSection, settingsNamespace } from '@deepseek-ai/dsh-settings'
import { MAX_TIMER_DELAY_MS } from '@deepseek-ai/dsh-timeout'
import { AiSidecarClient } from './sidecar.ts'
import type { SidecarConnection } from './sidecar.ts'
import { AiSdkAdapter, DEFAULT_STREAM_IDLE_TIMEOUT_MS } from './adapter.ts'
import type { SidecarApiKind, SidecarDiscoveredModel } from './types.ts'
import type {
  AiSdkCatalogModel,
  ResolvedRoute,
  ResolvedRouteSet,
} from './adapter.ts'

export { AiSdkAdapter, DEFAULT_STREAM_IDLE_TIMEOUT_MS } from './adapter.ts'
export type { AiSdkAdapterOptions, AiSdkCatalogModel, ResolvedRoute, ResolvedRouteSet } from './adapter.ts'
export { AiSidecarClient, DEFAULT_REQUEST_TIMEOUT_MS, SidecarProtocolError } from './sidecar.ts'
export { finishReasonOf, StreamAssembler, toChatRequest } from './translate.ts'
export type * from './types.ts'

export const name = 'llm-ai-sdk'
export const inject = ['llm']

const NS = settingsNamespace('llm-ai-sdk')
/** Environment variable naming the `ai-sidecar` executable. */
const BINARY_ENV = 'DSH_AI_SDK_SIDECAR'

/** The default provider route: DeepSeek's public endpoint. */
const DEFAULT_ROUTE = 'deepseek-official'

/** Public API default; other endpoints configure their own base URL. */
export const PUBLIC_BASE_URL = 'https://api.deepseek.com'
/** Default per-request output-token cap. */
export const DEFAULT_MAX_TOKENS = 256_000
/** Default combined request/response context capacity. */
const DEFAULT_CONTEXT_WINDOW = 1_000_000
/** Default bound on accumulated base64 image payload per request. */
export const DEFAULT_MAX_REQUEST_IMAGE_BYTES = 20 * 1024 * 1024
const DEFAULT_API_KEY_ENV = 'DEEPSEEK_API_KEY'

const DEFAULT_MODELS: AiSdkCatalogModel[] = [
  { id: 'deepseek-v4-flash', name: 'DeepSeek-V4-Flash', contextWindow: DEFAULT_CONTEXT_WINDOW },
  { id: 'deepseek-v4-pro', name: 'DeepSeek-V4-Pro', contextWindow: DEFAULT_CONTEXT_WINDOW },
]

const MODEL_MODALITIES = ['text', 'image'] as const satisfies readonly ModelModality[]

/**
 * Wire dialects a route may declare. `openai-compatible` serves every
 * OpenAI Chat Completions gateway including custom ids; `anthropic` and
 * `google` select the sidecar's native adapters for endpoints whose URL
 * alone cannot identify the format.
 */
export type RouteApiKind = SidecarApiKind

const API_KINDS: readonly RouteApiKind[] = ['openai-compatible', 'anthropic', 'google']

/** One provider profile on this plugin's config; the dict key is the route. */
export interface ProviderProfile {
  /** Selector label; defaults to the route key. */
  displayName?: string
  /**
   * Credential reference (environment-variable name) resolved per request;
   * omission derives `<ROUTE>_API_KEY` with non-alphanumeric runs replaced by
   * underscores.
   */
  apiKeyEnv?: string
  /** Endpoint base. Native SDK ids (anthropic, google) may omit it; OpenAI-compatible custom ids cannot. */
  baseURL?: string
  /**
   * Wire dialect of the endpoint; omission derives from the route id
   * (`anthropic` and `google` stay native, everything else speaks
   * OpenAI-compatible).
   */
  api?: RouteApiKind
  /** Advisory models shown by discovery consumers; requests remain unrestricted. */
  models?: AiSdkCatalogModel[]
  /** Default per-request output cap (default 256,000); explicit request values win. */
  maxTokens?: number
  /** Positive context capacity used when the selected model has no exact value (default 1,000,000). */
  defaultContextWindow?: number
  /** Reasoning efforts selectable for this route (default off/low/high/max). */
  reasoningEfforts?: ('off' | 'low' | 'high' | 'max')[]
  /**
   * Default thinking effort materialized into requests when callers omit one
   * (`high`); `off` disables thinking per request.
   */
  reasoningEffort?: 'off' | 'low' | 'high' | 'max'
  /** Maximum accumulated base64 image payload per request on this route (default 20 MiB). */
  maxRequestImageBytes?: number
}

/**
 * Plugin configuration. Every field is optional in yml: with no stored
 * configuration the plugin serves the default `deepseek-official` route
 * against the public endpoint with the `DEEPSEEK_API_KEY` reference.
 */
export interface Config {
  /**
   * Absolute path of the `ai-sidecar` executable. Falls back to
   * $DSH_AI_SDK_SIDECAR from a trusted environment layer. Unset everywhere,
   * the first request fails loud (`CONFIG`) with setup guidance — the web
   * Models page stays reachable so the path can be configured after boot.
   */
  binaryPath?: string
  /** Provider routes keyed by route id; omission keeps the default DeepSeek route. */
  providers?: Record<string, ProviderProfile>
  /** Maximum provider idle time while one stream read is outstanding (default five minutes). */
  streamIdleTimeoutMs?: number
  /** Shared model-request retry policy; omission uses normal mode defaults. */
  retryPolicy?: RetryPolicyConfig
}

const catalogModel = z.object({
  id: z.string().required(),
  name: z.string(),
  description: z.string(),
  contextWindow: z.number().step(1).min(1),
  maxTokens: z.number().step(1).min(1),
  // Schemastery materializes an absent array member as [], so the declared
  // default must live in the schema or omission would fail `.min(1)`.
  inputModalities: z.array(z.union(MODEL_MODALITIES)).min(1).default(['text']),
})

// Inferred rather than annotated `z<ProviderProfile>`: schemastery's inferred
// object type materializes every optional member, which fights
// exactOptionalPropertyTypes on the hand-written interface. The resolve step
// below is the typed boundary; the schema only shapes stored YAML.
const profileSchema = z.object({
  displayName: z.string(),
  apiKeyEnv: z.string(),
  baseURL: z.string(),
  api: z.union(API_KINDS),
  models: z.array(catalogModel),
  maxTokens: z.number().step(1).min(1).max(Number.MAX_SAFE_INTEGER),
  defaultContextWindow: z.number().step(1).min(1),
  reasoningEfforts: z.array(z.union(['off', 'low', 'high', 'max'])).min(1)
    .default(['off', 'low', 'high', 'max']),
  reasoningEffort: z.union(['off', 'low', 'high', 'max']),
  maxRequestImageBytes: z.number().step(1).min(1),
})

// Bridged rather than inferred: schemastery materializes optional members in
// its inferred object type, which fights exactOptionalPropertyTypes against
// the hand-written interface. `resolveAdapterOptions` is the typed boundary.
export const Config = z.object({
  binaryPath: z.string(),
  providers: z.dict(profileSchema),
  streamIdleTimeoutMs: z.number().min(Number.MIN_VALUE).max(MAX_TIMER_DELAY_MS)
    .default(DEFAULT_STREAM_IDLE_TIMEOUT_MS),
  retryPolicy: RetryPolicySchema,
}) as unknown as z<Config>

/** Validate one advisory catalog entry. */
function resolveModels(route: string, models: readonly AiSdkCatalogModel[]): AiSdkCatalogModel[] {
  const seen = new Set<string>()
  return models.map((model) => {
    if (model.id.length === 0) throw new Error(`llm-ai-sdk: catalog model ids must be non-empty on "${route}"`)
    if (seen.has(model.id)) throw new Error(`llm-ai-sdk: duplicate catalog model "${model.id}" on "${route}"`)
    seen.add(model.id)
    return {
      ...model.name === undefined ? {} : { name: model.name },
      ...model.description === undefined ? {} : { description: model.description },
      ...model.contextWindow === undefined ? {} : { contextWindow: model.contextWindow },
      ...model.maxTokens === undefined ? {} : { maxTokens: model.maxTokens },
      ...(model.inputModalities === undefined ? {} : { inputModalities: [...model.inputModalities] }),
      id: model.id,
    }
  })
}

/**
 * The dialect the sidecar derives for a route that declares none, mirroring
 * its id-based adapter selection exactly (`anthropic` and `google` stay
 * native; every other id speaks OpenAI-compatible).
 */
function defaultApiOf(route: string): RouteApiKind {
  return route === 'anthropic' || route === 'google' ? route : 'openai-compatible'
}

/** Whether the dialect has no SDK default endpoint, so a base URL is required. */
function baseURLRequired(api: RouteApiKind | undefined): boolean {
  return api === undefined || api === 'openai-compatible'
}

const KNOWN_MODALITIES: readonly ModelModality[] = ['text', 'image']

/**
 * Translate one sidecar model row into the discovery view, dropping rows the
 * wire boundary cannot vouch for (a listing row without an id names nothing).
 */
function discoveredOf(row: SidecarDiscoveredModel): LlmDiscoveredModel | undefined {
  if (typeof row.id !== 'string' || row.id.length === 0) return undefined
  const modalities = (row.capabilities?.input_modalities ?? [])
    .filter((value): value is ModelModality =>
      typeof value === 'string' && (KNOWN_MODALITIES as readonly string[]).includes(value))
  return {
    id: row.id,
    ...(typeof row.name === 'string' && row.name.length > 0 ? { name: row.name } : {}),
    ...(typeof row.context_window === 'number' && Number.isFinite(row.context_window)
      ? { contextWindow: row.context_window }
      : {}),
    ...(typeof row.max_output_tokens === 'number' && Number.isFinite(row.max_output_tokens)
      ? { maxTokens: row.max_output_tokens }
      : {}),
    ...(modalities.length > 0 ? { inputModalities: modalities } : {}),
  }
}

/**
 * The one explicit resolve step from raw config to validated routes. Called
 * for the composition entry at load (fail loud) and for each settings
 * snapshot at its first use.
 * @param config - raw plugin config or resolved settings snapshot.
 * @param environment - this run's environment layers, or `undefined` outside
 *   the product CLI.
 * @returns validated routes plus the shared sidecar binary path.
 */
export function resolveAdapterOptions(
  config: Config,
  environment?: LaunchEnvironmentSnapshot,
): ResolvedRouteSet & { streamIdleTimeoutMs: number } {
  const binaryPath = config.binaryPath ?? environment?.get(BINARY_ENV)?.value
  if (binaryPath !== undefined && binaryPath.trim().length === 0) {
    throw new Error('llm-ai-sdk: binaryPath must not be blank when set')
  }
  if (config.streamIdleTimeoutMs !== undefined
    && (!Number.isFinite(config.streamIdleTimeoutMs)
      || config.streamIdleTimeoutMs <= 0
      || config.streamIdleTimeoutMs > MAX_TIMER_DELAY_MS)) {
    throw new Error(
      `llm-ai-sdk: streamIdleTimeoutMs must be a positive finite number no greater than ${MAX_TIMER_DELAY_MS}`,
    )
  }
  const entries = Object.entries(config.providers ?? {})
  if (entries.length === 0) {
    entries.push([DEFAULT_ROUTE, {
      displayName: 'DeepSeek',
      apiKeyEnv: DEFAULT_API_KEY_ENV,
      baseURL: PUBLIC_BASE_URL,
      models: DEFAULT_MODELS.map(model => ({ ...model })),
      reasoningEfforts: ['off', 'low', 'high', 'max'],
      reasoningEffort: 'high',
    }])
  }
  const routes = new Map<string, ResolvedRoute>()
  for (const [route, profile] of entries) {
    if (profile.apiKeyEnv !== undefined && profile.apiKeyEnv.trim().length === 0) {
      throw new Error(`llm-ai-sdk: apiKeyEnv must not be blank on "${route}"`)
    }
    if (profile.baseURL !== undefined && profile.baseURL.trim().length === 0) {
      throw new Error(`llm-ai-sdk: baseURL must not be blank on "${route}"`)
    }
    if (profile.api !== undefined && !API_KINDS.includes(profile.api)) {
      throw new Error(`llm-ai-sdk: api must be one of ${API_KINDS.join(', ')} on "${route}"`)
    }
    if (profile.defaultContextWindow !== undefined
      && (!Number.isInteger(profile.defaultContextWindow) || profile.defaultContextWindow <= 0)) {
      throw new Error(`llm-ai-sdk: defaultContextWindow must be a positive integer on "${route}"`)
    }
    if (profile.maxTokens !== undefined
      && (!Number.isSafeInteger(profile.maxTokens) || profile.maxTokens <= 0)) {
      throw new Error(`llm-ai-sdk: maxTokens must be a positive safe integer on "${route}"`)
    }
    if (profile.maxRequestImageBytes !== undefined
      && (!Number.isSafeInteger(profile.maxRequestImageBytes) || profile.maxRequestImageBytes <= 0)) {
      throw new Error(`llm-ai-sdk: maxRequestImageBytes must be a positive safe integer on "${route}"`)
    }
    routes.set(route, {
      id: route,
      displayName: profile.displayName ?? route,
      // Same derivation as the web Models page's `deriveKeyRef`, so a route
      // declared there and a route defaulted here record one reference.
      apiKeyEnv: credentialRef(profile.apiKeyEnv ?? `${route.toUpperCase().replace(/[^A-Z0-9]+/g, '_')}_API_KEY`),
      baseURL: profile.baseURL,
      api: profile.api,
      models: resolveModels(route, profile.models ?? []),
      maxTokens: profile.maxTokens ?? DEFAULT_MAX_TOKENS,
      defaultContextWindow: profile.defaultContextWindow ?? DEFAULT_CONTEXT_WINDOW,
      reasoningEfforts: [...(profile.reasoningEfforts ?? ['off', 'low', 'high', 'max'])],
      defaultReasoningEffort: profile.reasoningEffort ?? 'high',
      maxRequestImageBytes: profile.maxRequestImageBytes ?? DEFAULT_MAX_REQUEST_IMAGE_BYTES,
    })
  }
  return {
    binaryPath: binaryPath ?? '',
    routes,
    streamIdleTimeoutMs: config.streamIdleTimeoutMs ?? DEFAULT_STREAM_IDLE_TIMEOUT_MS,
  }
}

export function apply(ctx: Context, config: Config): void {
  let current: () => Config = () => config
  let lastRaw: Config | undefined
  let lastGood: ReturnType<typeof resolveAdapterOptions> | undefined
  const options = (): ReturnType<typeof resolveAdapterOptions> => {
    const raw = current()
    if (raw === lastRaw && lastGood !== undefined) return lastGood
    try {
      const next = resolveAdapterOptions(raw, launchEnvironmentOf(ctx))
      lastRaw = raw
      lastGood = next
      return next
    } catch (error) {
      // Static composition resolves before anything registers, so this branch
      // only sees a live settings snapshot failing a beyond-schema bound:
      // keep serving the last good facts and say so once per bad snapshot.
      if (lastGood === undefined) throw error
      lastRaw = raw
      ctx.logger.error('llm-ai-sdk: keeping the last good configuration after an invalid settings section')
      ctx.logger.error(error)
      return lastGood
    }
  }
  options()

  const resolveApiKey = async (route: ResolvedRoute): Promise<string> => {
    // Every credential fact comes from the caller's snapshot, so a rejected
    // settings generation cannot leak its key onto the previous endpoint.
    const ref: CredentialRef = credentialRef(route.apiKeyEnv)
    const credentials = ctx.get('credentials')
    if (credentials !== undefined) {
      const hit = await credentials.resolve(ref)
      if (hit !== undefined) return assertUsableApiKey(hit.value, 'llm-ai-sdk', ref)
    } else {
      // Without the seam there is no managed store to rank against, so the
      // environment is the whole credential plane.
      const ambient = launchEnvironmentOf(ctx).get(ref)
      if (ambient !== undefined && ambient.value.length > 0) {
        return assertUsableApiKey(ambient.value, 'llm-ai-sdk', ref)
      }
    }
    throw new LlmError(
      `llm-ai-sdk: no API key for provider route "${route.id}"; store ${ref} through the credentials`
      + ` service (the web Models page writes it), or export ${ref} in the launching environment`,
      'MISSING_CREDENTIAL',
    )
  }

  /**
   * The auto-updater's managed install pointer: `core-deps/ai-sidecar/current.json`
   * beside the launch cwd, written by @deepseek-ai/dsh-sidecar-updates. Absent or
   * unreadable means "no managed install" — never an error.
   */
  const managedBinary = (): string | undefined => {
    try {
      const pointerPath = join(process.cwd(), 'core-deps', 'ai-sidecar', 'current.json')
      const pointer = JSON.parse(readFileSync(pointerPath, 'utf8')) as { exePath?: unknown }
      const exePath = typeof pointer.exePath === 'string' ? pointer.exePath : undefined
      if (exePath !== undefined && existsSync(exePath)) return exePath
    } catch {
      // No managed install (or a torn pointer mid-swap) is a normal state.
    }
    return undefined
  }

  const connection = (): SidecarConnection => {
    const resolved = options()
    const command = resolved.binaryPath.length > 0 ? resolved.binaryPath : managedBinary()
    if (command === undefined || command.trim().length === 0) {
      throw new LlmError(
        'llm-ai-sdk: no ai-sidecar binary is configured. Set llm-ai-sdk.binaryPath in cordis.yml or the'
        + ` ${NS} settings section, export $${BINARY_ENV} in the launching environment,`
        + ' or install the managed copy via the bell-icon update flow (core-deps/ai-sidecar).',
        'CONFIG',
      )
    }
    return { command, args: [] }
  }

  const sidecar = new AiSidecarClient(connection)
  ctx.effect(() => () => { sidecar.dispose() })

  /**
   * The stored credential of a configured route, when one resolves. Absence
   * is an answer, not a failure — keyless gateways (Ollama) are interrogated
   * without a bearer token — but a broken credentials seam must fail loud
   * rather than silently downgrade the interrogation to keyless.
   */
  const storedRouteKey = async (route: ResolvedRoute | undefined): Promise<string | undefined> => {
    if (route === undefined) return undefined
    try {
      return await resolveApiKey(route)
    } catch (error) {
      if (!(error instanceof LlmError) || error.code !== 'MISSING_CREDENTIAL') throw error
      return undefined
    }
  }

  const adapter = new AiSdkAdapter(
    {
      options: () => {
        const resolved = options()
        return { binaryPath: resolved.binaryPath, routes: resolved.routes }
      },
      resolveApiKey,
      resolveAttachments: (): AttachmentStore | undefined => ctx.get('attachments'),
    },
    sidecar,
    () => options().streamIdleTimeoutMs,
    () => resolveRetryPolicy(current().retryPolicy, 'llm-ai-sdk: retryPolicy'),
  )

  const directoryEntries = (): LlmConfigurableProvider[] =>
    [...options().routes.values()].map(route => ({
      provider: route.id,
      displayName: route.displayName,
      settingsNs: NS,
      settingsPath: ['providers', route.id],
      declared: true,
    }))
  const directoryRegistration = ctx.llm.registerConfigurableProviders(directoryEntries())
  const registration = ctx.llm.registerAdapter([...options().routes.keys()], adapter)

  /**
   * Interrogating an endpoint is a configuration-time action over a draft,
   * so the key arrives in the request (typed, not yet stored) or resolves
   * from the named route's stored credential; it is never persisted here.
   * A route the adapter already knows answers from its own catalog when the
   * draft names nothing new, costing no network call.
   */
  ctx.llm.registerModelDiscovery(NS, async (request) => {
    const resolved = options()
    const route = request.provider === undefined ? undefined : resolved.routes.get(request.provider)
    const requestedApi = request.api as RouteApiKind | undefined
    if (requestedApi !== undefined && !API_KINDS.includes(requestedApi)) {
      throw new LlmError(
        `llm-ai-sdk: unknown wire protocol ${JSON.stringify(request.api)};`
        + ` expected one of ${API_KINDS.join(', ')}`,
        'CONFIG',
      )
    }
    const api: RouteApiKind | undefined = requestedApi
      ?? route?.api
      ?? (route === undefined ? undefined : defaultApiOf(route.id))
    // A draft naming neither endpoint nor protocol override on a known route
    // asks "what do you already know about this route" — answered from the
    // advisory catalog without touching the sidecar.
    if (route !== undefined && request.baseURL === undefined && request.apiKey === undefined && requestedApi === undefined) {
      return route.models.map(model => ({
        id: model.id,
        ...(model.name === undefined ? {} : { name: model.name }),
        ...(model.contextWindow === undefined ? {} : { contextWindow: model.contextWindow }),
        ...(model.maxTokens === undefined ? {} : { maxTokens: model.maxTokens }),
        ...(model.inputModalities === undefined ? {} : { inputModalities: [...model.inputModalities] }),
      }))
    }
    const apiKey = request.apiKey ?? await storedRouteKey(route)
    const resolvedBaseURL = request.baseURL ?? route?.baseURL
    if (baseURLRequired(api) && resolvedBaseURL === undefined) {
      throw new LlmError(
        'llm-ai-sdk: model discovery needs a baseURL for OpenAI-compatible endpoints'
        + ' — fill in the endpoint or pick a native protocol',
        'CONFIG',
      )
    }
    const rows: SidecarDiscoveredModel[] = await sidecar.discoverModels({
      ...(apiKey === undefined ? {} : { apiKey }),
      ...(resolvedBaseURL === undefined ? {} : { baseURL: resolvedBaseURL }),
      ...(api === undefined ? {} : { api }),
    })
    return rows.flatMap((row) => {
      const discovered = discoveredOf(row)
      return discovered === undefined ? [] : [discovered]
    })
  })

  installSettingsSection(ctx, NS, Config, config, {
    setSource: (source) => {
      current = source
    },
    onChange: () => {
      // Route membership follows the live resolution; replace atomically so
      // observers never see an empty route set between dispose and register.
      // The directory rides its own replace handle: a bare re-registration
      // here would collide with the ids this plugin already declares and
      // tear the whole settings wiring down mid-attach.
      registration.replace([...options().routes.keys()])
      directoryRegistration.replace(directoryEntries())
    },
  })
}
