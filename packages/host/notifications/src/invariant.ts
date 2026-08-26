/**
 * Package-owned invariant companion for the notification seam: the
 * `notifications/updated` and `notifications/removed` event streams must agree
 * with the service's authoritative store at dispatch time.
 * @module @deepseek-ai/dsh-notifications/invariant
 */

import type { Context } from '@deepseek-ai/cordis'
import type { InvariantFailure, InvariantInstaller } from '@deepseek-ai/dsh-invariants'

const PACKAGE_NAME = '@deepseek-ai/dsh-notifications'

/** Cordis companion plugin name. */
export const name = 'notifications-invariant'
/** Service required before the companion can register. */
export const inject = ['invariants']

/**
 * Install the seam contribution into its child registration fiber. The
 * listener reads the authoritative store through the child fiber, so the
 * notification service is declared as its injection.
 */
const install: InvariantInstaller = Object.assign((ctx: Context, fail: InvariantFailure) => {
  ctx.on('notifications/updated', (id) => {
    if (!ctx.notifications.list().some(view => view.id === id)) {
      fail(`notifications/updated carried "${id}" but the service does not hold it`)
    }
  })
  ctx.on('notifications/removed', (id) => {
    if (ctx.notifications.list().some(view => view.id === id)) {
      fail(`notifications/removed carried "${id}" but the service still holds it`)
    }
  })
}, { inject: ['notifications'] })

/**
 * Register the notification-seam invariant companion.
 * @param ctx - Cordis context carrying the invariant service.
 * @returns the installed registration's disposer after setup succeeds.
 */
export const apply = (ctx: Context): Promise<() => void> =>
  Promise.resolve(ctx.invariants.register(PACKAGE_NAME, install))
