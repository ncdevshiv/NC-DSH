/**
 * Package-owned invariant companion for the sidecar-update pipeline: every
 * committed status whose pointer names an install must point at a binary that
 * exists on disk, because installs write the executable before the pointer
 * rename commits.
 * @module @deepseek-ai/dsh-sidecar-updates/invariant
 */

import { existsSync } from 'node:fs'
import type { Context } from '@deepseek-ai/cordis'
import type { InvariantInstaller } from '@deepseek-ai/dsh-invariants'

const PACKAGE_NAME = '@deepseek-ai/dsh-sidecar-updates'

/** Cordis companion plugin name. */
export const name = 'sidecar-updates-invariant'
/** Service required before the companion can register. */
export const inject = ['invariants']

/** Install the pipeline contribution into its child registration fiber. */
const install: InvariantInstaller = (ctx, fail) => {
  ctx.on('sidecar-updates/status', (status) => {
    if (status.installed !== null && !existsSync(status.installed.exePath)) {
      fail(`status.installed points at "${status.installed.exePath}", which does not exist on disk`)
    }
  })
}

/**
 * Register the sidecar-updates invariant companion.
 * @param ctx - Cordis context carrying the invariant service.
 * @returns the installed registration's disposer after setup succeeds.
 */
export const apply = (ctx: Context): Promise<() => void> =>
  Promise.resolve(ctx.invariants.register(PACKAGE_NAME, install))
