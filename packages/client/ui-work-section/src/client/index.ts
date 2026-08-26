/**
 * Work section plugin, browser half. Registers the live work board into the
 * sidebar shell's keyed `sidebar.section` slot under the `work` key. All
 * data rides the standard session-list hook (needs-you, running, and the
 * materialized goal projections); the registration waits on the shell's
 * declaration through `slots.inject()` and leaves with the caller's fiber.
 */
import type { ClientContext } from '@deepseek-ai/dsh-client-runtime/client'
// Type-only: pulls the locale plugin's Context merge (ctx.locale).
import type {} from '@deepseek-ai/dsh-client-locale/client'
import type { WorkSectionInjected } from './WorkSection.tsx'
import { WorkSection } from './WorkSection.tsx'
import { en, zh, type WorkKey } from './locales.ts'

export type { WorkSectionInjected, WorkSectionProps } from './WorkSection.tsx'
export type { WorkKey } from './locales.ts'

declare module '@deepseek-ai/dsh-client-ui-slots' {
  interface LocaleNamespaceMap {
    /** The Work section's board copy. */
    work: WorkKey
  }
}

/** Dictionary namespace owned by this plugin. */
const NS = 'work'

/** Services required by the Work section plugin. */
export const inject = ['slots', 'sessions', 'locale']

/**
 * Register the Work section once the sidebar's keyed declaration is on the
 * ledger.
 * @param ctx - client root context.
 */
export function apply(ctx: ClientContext): void {
  ctx.effect(() => ctx.locale.register(NS, { zh, en }), 'ui-work-section: dictionaries')

  const injected = (): WorkSectionInjected => ({
    open: (sessionId) => { ctx.sessions.open(sessionId) },
  })
  ctx.slots.inject('sidebar.section', () => ctx.slots.register(
    {
      name: 'sidebar.section',
      key: 'work',
      inject: injected,
      locale: NS,
    },
    WorkSection,
  ))
}
