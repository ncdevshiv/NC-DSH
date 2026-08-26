/**
 * Package-owned invariant companion for `@deepseek-ai/dsh-client-ui-notifications`.
 * @module @deepseek-ai/dsh-client-ui-notifications/invariant
 */

/* jscpd:ignore-start */
import type { Context } from '@deepseek-ai/cordis'
import type { InvariantInstaller } from '@deepseek-ai/dsh-invariants'

const PACKAGE_NAME = '@deepseek-ai/dsh-client-ui-notifications'

/** Cordis companion plugin name. */
export const name = 'client-ui-notifications-invariant'
/** Service required before the companion can reserve package ownership. */
export const inject = ['invariants']

/**
 * No runtime invariant: this package is a read-mostly projection of the
 * `notifications`/`updates` wire domains onto one sidebar-footer slot entry.
 * It emits no cordis events and owns no cross-plugin mutable state; its single
 * slot registration proves disposal through the HMR-safety spec, and every
 * write goes through the wire domains that own the durable facts.
 */
const install: InvariantInstaller = () => {}

/**
 * Register this package's invariant companion.
 * @param ctx - Cordis context carrying the invariant service.
 * @returns the installed registration's disposer after setup succeeds.
 */
export const apply = (ctx: Context): Promise<() => void> =>
  Promise.resolve(ctx.invariants.register(PACKAGE_NAME, install))
/* jscpd:ignore-end */
