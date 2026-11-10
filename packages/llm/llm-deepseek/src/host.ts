/** Shared Host wiring for the DeepSeek protocol adapter. */
import type { Context } from '@deepseek-ai/cordis'
import type {} from '@deepseek-ai/dsh-settings'
import type {} from '@deepseek-ai/cordis-plugin-loader'
import type {} from '@deepseek-ai/dsh-fs'
import { resolveImageAttachmentAccess } from '@deepseek-ai/dsh-llm'
import { getOrCreateAnonymousUserId, type AnonymousUserId } from '@deepseek-ai/dsh-anonymous-user-id'
import { deepEqualJson } from '@deepseek-ai/dsh-util-values'
import { DeepSeekAdapter } from './adapter.ts'
import { testDeepSeekModel } from './connectivity.ts'
import type { DeepSeekAdapterOptions, DeepSeekConnectionOptions } from './types.ts'

/**
 * Register one provider with request-local transport services and live retry policy.
 * @param ctx - provider plugin lifetime with the LLM registry injected.
 * @param provider - exact route owned by this plugin.
 * @param dependencies - provider-owned discovery, credential, and configuration callbacks.
 */
export function registerDeepSeekProvider<C extends DeepSeekConnectionOptions>(
  ctx: Context, provider: string, dependencies: Pick<DeepSeekAdapterOptions<C>,
  'options' | 'resolveAuth' | 'providerName' | 'discoverModels' | 'connect' | 'admit'>): void {
  ctx.inject(['settings'], (child) => { child.effect(() => child.settings.configure({ auto: false }, ctx.fiber)) })
  let userId: AnonymousUserId | undefined
  const resolveUserId = (): AnonymousUserId => userId ??= getOrCreateAnonymousUserId()
  // These conditions repeat on every request of a route; log each first occurrence only.
  const degradedReplays = new Set<string>()
  const omittedRoutes = new Set<string>()
  const unacceptedRoutes = new Set<string>()
  const adapter = new DeepSeekAdapter<C>({
    ...dependencies,
    resolveUserId,
    onReplayDegrade: ({ provider, model, reason }) => {
      const key = `${provider}/${model}\u0000${reason}`
      if (degradedReplays.has(key)) return
      degradedReplays.add(key)
      ctx.logger.warn(`llm-deepseek: unusable Messages replay state on assistant history for route "${provider}/${model}"; sending provider-neutral content (${reason})`)
    },
    onExtensionsOmitted: ({ provider, model, fields, error }) => {
      const route = `${provider}/${model}`
      if (omittedRoutes.has(route)) return
      omittedRoutes.add(route)
      ctx.logger.warn(`llm-deepseek: sending route "${route}" without request extension fields ${fields.join(', ')} because they failed to serialize: %o`, error)
    },
    onExtensionsUnaccepted: ({ provider, model, error }) => {
      const route = `${provider}/${model}`
      if (unacceptedRoutes.has(route)) return
      unacceptedRoutes.add(route)
      ctx.logger.warn(`llm-deepseek: route "${route}" request extension acceptance failed; contributors resend on a later request: %o`, error)
    },
    resolveAttachments: () => ctx.get('attachments'),
    resolveImageAccess: (attachments, ref) => resolveImageAttachmentAccess(
      attachments, hostPath => ctx.get('fs')?.processPathFromHostPath(hostPath), ref,
    ),
    prepareExtensions: request => ctx.get('deepseekLlmApiExtensions')?.prepare(request)
      ?? Promise.resolve({ fields: {}, accept: () => Promise.resolve() }),
  })
  const registration = ctx.llm.registerAdapter([provider], adapter)
  // A model probe is a configuration-time action over a draft, so it is offered
  // for the namespace that owns this route's settings section rather than per
  // route: the endpoint a surface is testing may not register yet. A
  // hand-mounted composition has no entry id, so the plugin's own name keys the
  // offer — the same fallback each DeepSeek consumer reports as its settings
  // namespace.
  const settingsNs = ctx.fiber.entry?.options.id ?? ctx.fiber.name
  ctx.llm.registerModelTest(settingsNs, (request, signal) => testDeepSeekModel(
    { ...request, ...signal === undefined ? {} : { signal } },
    {
      seam: ctx.llm,
      options: dependencies.options,
      routeAuth: () => dependencies.resolveAuth(dependencies.options()),
      resolveUserId,
    },
  ))
  /**
   * The route set and retry policy the registry currently holds. `enabled`
   * rides with the policy because both are read from the same snapshot: a
   * paused route holds zero routes rather than serving requests the deployment
   * turned off. Withdrawal uses `replace([])` instead of releasing the
   * registration, so re-enabling stays inside this fiber's lifetime and the
   * first registration still unwinds with the plugin.
   */
  let registered = { enabled: true, retryPolicy: dependencies.options().retryPolicy }
  const sync = (): void => {
    const options = dependencies.options()
    const next = { enabled: options.enabled, retryPolicy: options.retryPolicy }
    if (deepEqualJson(next, registered)) return
    registration.replace(next.enabled ? [provider] : [])
    registered = next
  }
  sync()
  ctx.on('loader/volatile-update', () => {
    try { sync() }
    catch (error) { ctx.logger.warn(error) }
  })
}
