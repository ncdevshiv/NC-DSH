/**
 * Package-owned invariant companion for `@deepseek-ai/dsh-tool-browser`.
 * @module @deepseek-ai/dsh-tool-browser/invariant
 */

/* jscpd:ignore-start */
import type { Context } from '@deepseek-ai/cordis'
import type { InvariantInstaller } from '@deepseek-ai/dsh-invariants'

const PACKAGE_NAME = '@deepseek-ai/dsh-tool-browser'

/** Cordis companion plugin name. */
export const name = 'tool-browser-invariant'
/** Service required before the companion can reserve package ownership. */
export const inject = ['invariants']

/**
 * No runtime invariant: the shared session's lifecycle is private to the
 * holder and enforced on every operation; tool outputs are pure functions of
 * seam results with no independent observation stream.
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
