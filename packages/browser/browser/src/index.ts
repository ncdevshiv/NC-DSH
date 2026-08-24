/**
 * Service Definition for the browser-automation capability seam (`ctx.browser`): a provider
 * registry and provider-selecting session launch. Duplicate ids are rejected. At launch time, a
 * configured provider must exist and be usable; without one, exactly one usable provider is
 * required, so selection never depends on registration order. Selection mirrors the web seam so
 * both capabilities behave identically from the deployment's point of view.
 * @module @deepseek-ai/dsh-browser
 */

import { Context, Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {
  BrowserProvider,
  BrowserSession,
} from './types.ts'
import { BrowserError } from './types.ts'

export {
  BrowserError,
} from './types.ts'
export type {
  BrowserClickRequest,
  BrowserNavigateRequest,
  BrowserPageState,
  BrowserProvider,
  BrowserScreenshot,
  BrowserScreenshotRequest,
  BrowserSession,
  BrowserTypeRequest,
} from './types.ts'

declare module '@deepseek-ai/cordis' {
  interface Context {
    browser: BrowserRuntime
  }
}

/** Selection inputs for launch-time provider resolution. */
interface Selection<P> {
  /** The configured provider id, if any. */
  readonly configuredId?: string
  /** Providers registered for this seam. */
  readonly providers: ReadonlyMap<string, P>
}

/**
 * Config for the browser seam. `provider` pins which registered backend wins; it is optional (a
 * single registered usable provider auto-selects). Operational overrides feed this same field
 * rather than introduce a hidden priority chain.
 */
export interface BrowserRuntimeConfig {
  /** Explicit provider id. Omitted = auto-select when exactly one usable. */
  readonly provider?: string
}

/**
 * The browser-automation service. Registered as `ctx.browser` (one instance per context).
 *
 * Selection semantics (resolved at launch time, never order-dependent):
 * - A configured id that is registered and `available()` → that provider.
 * - A configured id not registered → `BROWSER_PROVIDER_CONFIGURED_MISSING`.
 * - A configured id registered but unavailable →
 *   `BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE`.
 * - No id configured, exactly one registered usable provider → that provider.
 * - No id configured, multiple usable providers → `BROWSER_PROVIDER_AMBIGUOUS`.
 * - No id configured, no usable provider → `BROWSER_PROVIDER_UNAVAILABLE`.
 */
export class BrowserRuntime extends Service {
  /**
   * Provider selection config. `$DSH_BROWSER_PROVIDER` is equivalent to the
   * `provider` field and is NOT a hidden priority chain — the env var feeds the
   * same resolved value.
   */
  static Config: z<BrowserRuntimeConfig> = z.object({
    provider: z.string(),
  })

  private providers = new Map<string, BrowserProvider>()
  private readonly providerId: string | undefined

  constructor(ctx: Context, config: BrowserRuntimeConfig = {}) {
    super(ctx, 'browser')
    this.providerId = config.provider ?? process.env.DSH_BROWSER_PROVIDER
  }

  /**
   * Register a browser provider. Throws {@link BrowserError} `BROWSER_DUPLICATE_PROVIDER`
   * if its id is already registered. Returns a disposer; disposed with the calling fiber.
   * @param provider - the provider; its `id` is the registry key.
   * @returns the disposer that unregisters the provider.
   */
  registerProvider(provider: BrowserProvider): () => void {
    const store = this.providers
    if (store.has(provider.id)) {
      throw new BrowserError(`a browser provider with id "${provider.id}" is already registered`, 'BROWSER_DUPLICATE_PROVIDER')
    }
    const dispose = this.ctx.effect(function* () {
      store.set(provider.id, provider)
      yield () => store.delete(provider.id)
    }, 'browser.registerProvider()')
    // ctx.effect's disposer returns Promise<void>; our disposer API is
    // synchronous fire-and-forget — discard the (always-resolved) promise.
    return () => void dispose()
  }

  /**
   * Launch a session through the selected provider. Resolves the provider at call time with the
   * selection rules above; throws {@link BrowserError} when no provider can run. The caller owns
   * the returned session and must eventually call its `close()`.
   * @param signal - optional cancellation signal for the startup phase.
   * @returns the launched session.
   */
  async launch(signal?: AbortSignal): Promise<BrowserSession> {
    const provider = resolveProvider({
      providers: this.providers,
      ...this.providerId !== undefined ? { configuredId: this.providerId } : {},
    })
    return provider.launch(signal)
  }
}

/* jscpd:ignore-start -- selection deliberately mirrors the web seam so both registries
   behave identically; the copies evolve only together by review. */
interface ResolvableProvider {
  readonly id: string
  available(): boolean
}

/** Resolve the selected provider or throw the matching {@link BrowserError}. */
function resolveProvider<P extends ResolvableProvider>(selection: Selection<P>): P {
  const { configuredId, providers } = selection
  if (configuredId !== undefined) {
    const provider = providers.get(configuredId)
    if (!provider) {
      throw new BrowserError(`configured browser provider "${configuredId}" is not registered`, 'BROWSER_PROVIDER_CONFIGURED_MISSING')
    }
    if (!provider.available()) {
      throw new BrowserError(`configured browser provider "${configuredId}" is registered but unavailable`, 'BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE')
    }
    return provider
  }
  const usable = [...providers.values()].filter(provider => provider.available())
  const [single] = usable
  if (single === undefined) {
    throw new BrowserError('no usable browser provider is registered', 'BROWSER_PROVIDER_UNAVAILABLE')
  }
  if (usable.length > 1) {
    const ids = usable.map(provider => provider.id).join(', ')
    throw new BrowserError(`multiple usable browser providers are registered (${ids}); configure one explicitly`, 'BROWSER_PROVIDER_AMBIGUOUS')
  }
  return single
}
/* jscpd:ignore-end */

export default BrowserRuntime
