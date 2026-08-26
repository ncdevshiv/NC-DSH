/**
 * Notifications plugin, browser half: contributes the sidebar-foot bell entry
 * (system notices plus the AI SDK update card) over the frozen
 * notifications/updates wire faces read off the connection handle. Export
 * discipline: packages/client/AGENTS.md.
 */
import type { ClientContext } from '@deepseek-ai/dsh-client-runtime/client'
// Type-only: pulls the locale plugin's Context merge (ctx.locale).
import type {} from '@deepseek-ai/dsh-client-locale/client'
import { NotificationsBell } from './NotificationsBell.tsx'
import type { NotificationsInjected } from './NotificationsBell.tsx'
import {
  createNotificationsStore,
  type UpdatesFace,
  type NotificationsFace,
} from './store.ts'
import { en, zh, type NotificationsKey } from './locales.ts'

export type { NotificationsBellProps, NotificationsInjected } from './NotificationsBell.tsx'
export type { NotificationsController } from './store.ts'
export type { NotificationsKey } from './locales.ts'

declare module '@deepseek-ai/dsh-client-ui-slots' {
  interface LocaleNamespaceMap {
    /** Notification-center copy. */
    'notifications': NotificationsKey
  }
}

/** Dictionary namespace owned by this plugin. */
const NS = 'notifications'

/**
 * Narrow slice of the connection handle this plugin reads. The wire contract
 * is frozen while the gateway lands in parallel, so the faces stay local
 * structural types; when IApiClient declares the domains, this cast is
 * replaced by `Pick<IApiClient, 'updates' | 'notifications'>` verbatim.
 */
interface NotificationsConnection {
  /** The API gateway's client face, sliced to the two domains below. */
  api: UpdatesFace & NotificationsFace
}

/** Required services (cordis fiber inject). */
export const inject = ['slots', 'locale', 'connection']

/**
 * Client plugin body: register the dictionaries and the sidebar-foot bell.
 * Registration depends on ui-sidebar's `sidebar.footer.action` declaration
 * through `slots.inject()`, so activation order stays unconstrained.
 * @param ctx - client root context.
 */
export function apply(ctx: ClientContext): void {
  ctx.effect(() => ctx.locale.register(NS, { zh, en }), 'ui-notifications: dictionaries')

  const api = (ctx.get('connection') as NotificationsConnection).api
  const controller = createNotificationsStore(api)

  ctx.slots.inject('sidebar.footer.action', () => ctx.slots.register({
    name: 'sidebar.footer.action',
    id: 'notifications-bell',
    // Below the Cordis panel row, directly beside the settings seat.
    order: 30,
    locale: NS,
    inject: (): NotificationsInjected => ({
      hooks: { snapshot: controller.store },
      controller,
    }),
  }, NotificationsBell))
}
