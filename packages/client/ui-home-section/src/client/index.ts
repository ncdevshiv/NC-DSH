/**
 * Home section plugin, browser half. Registers the inbox overview into the
 * sidebar shell's keyed `sidebar.section` slot under the `home` key. The
 * registration waits on the shell's declaration through `slots.inject()`
 * (apply order is unconstrained); the inject factory closes over ctx and
 * hands the component the runtime's shared session verbs.
 */
import type { ClientContext } from '@deepseek-ai/dsh-client-runtime/client'
// Type-only: pulls the locale plugin's Context merge (ctx.locale).
import type {} from '@deepseek-ai/dsh-client-locale/client'
import type { HomeSectionInjected } from './HomeSection.tsx'
import { HomeSection } from './HomeSection.tsx'
import { en, zh, type HomeKey } from './locales.ts'

export type { HomeSectionInjected, HomeSectionProps } from './HomeSection.tsx'
export type { HomeKey } from './locales.ts'

declare module '@deepseek-ai/dsh-client-ui-slots' {
  interface LocaleNamespaceMap {
    /** The Home section's overview copy. */
    home: HomeKey
  }
}

/** Dictionary namespace owned by this plugin. */
const NS = 'home'

/** Services required by the Home section plugin. */
export const inject = ['slots', 'sessions', 'workspaces', 'locale']

/**
 * Register the Home section once the sidebar's keyed declaration is on the
 * ledger.
 * @param ctx - client root context.
 */
export function apply(ctx: ClientContext): void {
  ctx.effect(() => ctx.locale.register(NS, { zh, en }), 'ui-home-section: dictionaries')

  const injected = (): HomeSectionInjected => ({
    startSession: () => ctx.workspaces.startSession(),
    open: (sessionId) => { ctx.sessions.open(sessionId) },
  })
  ctx.slots.inject('sidebar.section', () => ctx.slots.register(
    {
      name: 'sidebar.section',
      key: 'home',
      inject: injected,
      locale: NS,
    },
    HomeSection,
  ))
}
