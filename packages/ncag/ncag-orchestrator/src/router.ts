/**
 * Model route discovery and selection.
 *
 * NC-AG never invents a model: routes come from the harness provider registry,
 * and a profile's capability tier is resolved to a concrete route by the
 * configured policy. When the registry advertises nothing, the deployment
 * must configure an explicit route, and the resolver fails loudly instead of
 * silently choosing one.
 * @module @deepseek-ai/dsh-ncag-orchestrator/router
 */

import type { ModelRoute, ModelTier } from '@deepseek-ai/dsh-ncag-core'

/** One price entry for one exact model route, in US dollars per million tokens. */
export interface ModelPrice {
  readonly provider: string
  readonly model: string
  readonly inputPerMillion: number
  readonly outputPerMillion: number
  /** Tier override for this explicit route. */
  readonly tier?: ModelTier
}

/** Provider metadata the resolver needs from the harness LLM service. */
export interface ProviderInfo {
  readonly id: string
  readonly name: string
}

/** Model metadata the resolver needs from the harness LLM service. */
export interface ModelInfo {
  readonly id: string
  readonly name: string
}

/** Registry surface the resolver reads. */
export interface RouteRegistry {
  /**
   * List registered provider routes.
   * @returns the providers with an adapter.
   */
  listProviders(): readonly ProviderInfo[]
  /**
   * List models one provider advertises.
   * @param provider - provider route id.
   * @returns the model metadata.
   */
  listModels(provider: string): Promise<readonly ModelInfo[]>
}

/** Resolution request for one assignment. */
export interface RouteRequest {
  /** Tier the profile prefers. */
  readonly tier: ModelTier
  /** Preferred provider names, in order. */
  readonly preferredProviders: readonly string[]
  /** Selection policy. */
  readonly policy: 'economical' | 'balanced' | 'capability-first'
  /**
   * The user's main model, used for every tier until an override names a
   * route for that tier. Absent when the deployment configures no default
   * and the harness advertises no route.
   */
  readonly defaultRoute?: ModelRoute
  /** Explicit route for this tier, from the NC-AG settings; wins over the default. */
  readonly override?: ModelRoute
}

/** One resolvable route with its optional price and tier. */
export interface ResolvedRoute extends ModelRoute {
  readonly tier: ModelTier
  /** USD per million input tokens, or null when the deployment did not price this route. */
  readonly inputPerMillion: number | null
  /** USD per million output tokens, or null when the deployment did not price this route. */
  readonly outputPerMillion: number | null
}

/** Heuristic tier for a model id the deployment did not price explicitly. */
function tierFromModelId(model: string): ModelTier {
  const id = model.toLowerCase()
  if (id.includes('reason') || id.includes('think') || id.startsWith('r1') || id.includes('o3') || id.includes('pro')) return 'reasoning'
  if (id.includes('flash') || id.includes('mini') || id.includes('lite') || id.includes('small')) return 'fast'
  return 'standard'
}

/**
 * Resolves capability tiers to concrete provider routes.
 */
export class ModelRouter {
  private readonly registry: RouteRegistry
  private prices: readonly ModelPrice[]
  private routes: ResolvedRoute[] | undefined

  /**
   * @param registry - the provider and model registry.
   * @param prices - explicitly priced routes.
   */
  constructor(registry: RouteRegistry, prices: readonly ModelPrice[]) {
    this.registry = registry
    this.prices = prices
  }

  /**
   * Replace the price overrides; the next discovery rebuilds the route set.
   * @param prices - the current deployment price overrides.
   */
  setPrices(prices: readonly ModelPrice[]): void {
    const next = JSON.stringify(prices)
    if (next === JSON.stringify(this.prices)) return
    this.prices = prices
    this.routes = undefined
  }

  /**
   * Discover every route the deployment can serve, including the default route
   * the user's main model supplies when it is not already priced.
   * @param defaultRoute - the user's main model, used when no override names a route.
   * @returns the routes, cheapest first; unpriced routes sort last.
   * @throws Error when no route exists at all.
   */
  async discover(defaultRoute?: ModelRoute): Promise<ResolvedRoute[]> {
    if (this.routes === undefined) {
      const discovered: ResolvedRoute[] = []
      for (const price of this.prices) {
        discovered.push({
          provider: price.provider,
          model: price.model,
          tier: price.tier ?? tierFromModelId(price.model),
          inputPerMillion: price.inputPerMillion,
          outputPerMillion: price.outputPerMillion,
        })
      }
      for (const provider of this.registry.listProviders()) {
        let models: readonly ModelInfo[] = []
        try {
          models = await this.registry.listModels(provider.id)
        } catch {
          models = []
        }
        for (const model of models) {
          if (discovered.some(route => route.provider === provider.id && route.model === model.id)) continue
          discovered.push({
            provider: provider.id,
            model: model.id,
            tier: tierFromModelId(model.id),
            inputPerMillion: null,
            outputPerMillion: null,
          })
        }
      }
      if (defaultRoute !== undefined
        && !discovered.some(route => route.provider === defaultRoute.provider && route.model === defaultRoute.model)) {
        discovered.push({
          provider: defaultRoute.provider,
          model: defaultRoute.model,
          tier: tierFromModelId(defaultRoute.model),
          inputPerMillion: null,
          outputPerMillion: null,
        })
      }
      if (discovered.length === 0) {
        throw new Error('ncag: no model route is configured. Register a harness LLM provider or select a main model in the NC-AG settings.')
      }
      discovered.sort((a, b) =>
        (a.inputPerMillion ?? Number.POSITIVE_INFINITY) - (b.inputPerMillion ?? Number.POSITIVE_INFINITY)
        || (a.outputPerMillion ?? Number.POSITIVE_INFINITY) - (b.outputPerMillion ?? Number.POSITIVE_INFINITY))
      this.routes = discovered
    }
    return this.routes
  }

  /**
   * Resolve one assignment to a concrete route.
   *
   * A deployment override for the requested tier wins; otherwise the user's
   * main model serves every tier. Unpriced routes are allowed: token counts
   * are always recorded, and only a priced route contributes USD.
   * @param request - tier, preferred providers, policy, and the main-model default.
   * @returns the chosen route.
   * @throws Error when no route satisfies the request.
   */
  async resolve(request: RouteRequest): Promise<ResolvedRoute> {
    const routes = await this.discover(request.defaultRoute)
    const explicit = request.override
    if (explicit !== undefined) {
      const found = routes.find(route => route.provider === explicit.provider && route.model === explicit.model)
      if (found !== undefined) return found
      const added: ResolvedRoute = {
        provider: explicit.provider,
        model: explicit.model,
        tier: tierFromModelId(explicit.model),
        inputPerMillion: null,
        outputPerMillion: null,
      }
      routes.push(added)
      return added
    }
    const preferred = request.preferredProviders
    const rank = (route: ResolvedRoute): number => {
      const index = preferred.indexOf(route.provider)
      return index < 0 ? preferred.length : index
    }
    const tierMatch = (route: ResolvedRoute): number => {
      if (route.tier === request.tier) return 0
      if (request.tier === 'reasoning') return route.tier === 'standard' ? 1 : 2
      if (request.tier === 'standard') return route.tier === 'reasoning' ? 1 : 2
      return route.tier === 'standard' ? 1 : 2
    }
    const isDefault = (route: ResolvedRoute): boolean =>
      request.defaultRoute !== undefined
      && route.provider === request.defaultRoute.provider && route.model === request.defaultRoute.model
    const ordered = [...routes].sort((a, b) =>
      (isDefault(b) ? 1 : 0) - (isDefault(a) ? 1 : 0)
      || rank(a) - rank(b) || tierMatch(a) - tierMatch(b)
      || (request.policy === 'capability-first'
        ? 0
        : (a.outputPerMillion ?? Number.POSITIVE_INFINITY) - (b.outputPerMillion ?? Number.POSITIVE_INFINITY)))
    const chosen = ordered[0]
    /* v8 ignore next -- discover() refuses an empty route set, so a route is always chosen. */
    if (chosen === undefined) throw new Error('ncag: no model route matches the requested capability tier')
    return chosen
  }

  /**
   * Price one completed call.
   * @param route - the route the call used.
   * @param inputTokens - uncached input tokens.
   * @param cachedTokens - cache-read tokens.
   * @param outputTokens - output tokens.
   * @returns the cost in US dollars, or null when the route is unpriced.
   */
  static price(route: ResolvedRoute, inputTokens: number, cachedTokens: number, outputTokens: number): number | null {
    if (route.inputPerMillion === null || route.outputPerMillion === null) return null
    const inputCost = ((inputTokens + cachedTokens) / 1_000_000) * route.inputPerMillion
    const outputCost = (outputTokens / 1_000_000) * route.outputPerMillion
    return Number((inputCost + outputCost).toFixed(6))
  }
}
