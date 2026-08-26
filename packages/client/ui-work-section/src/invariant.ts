/**
 * Package-owned invariant companion for `@deepseek-ai/dsh-client-ui-work-section`.
 * @module @deepseek-ai/dsh-client-ui-work-section/invariant
 */

/* jscpd:ignore-start */
import type { Context } from '@deepseek-ai/cordis'
import type { InvariantInstaller } from '@deepseek-ai/dsh-invariants'

const PACKAGE_NAME = '@deepseek-ai/dsh-client-ui-work-section'

/** Cordis companion plugin name. */
export const name = 'client-ui-work-section-invariant'
/** Service required before the companion can reserve package ownership. */
export const inject = ['invariants']

/**
 * No runtime invariant: a pure-consumer plugin deriving members in-component
 * from the standard useSessions delivery and reading the preset roster
 * through one snapshot store fed by the agentPresets wire face — it emits no
 * cordis events and owns no cross-plugin mutable state; derivation and
 * interaction behavior are asserted directly by this package's component
 * specs.
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
