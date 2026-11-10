/**
 * NC-AG tool plugin: it registers the platform's model-facing tools on the tool
 * registry. The platform service is mounted by the ncag-service package.
 * Named exports preserve the Loader metadata.
 * @module @deepseek-ai/dsh-tool-ncag
 */

import type { Context } from '@deepseek-ai/cordis'
import { createNcagTools } from './tools.ts'

export { createNcagTools } from './tools.ts'

/** Plugin name. */
export const name = 'tool-ncag'

/** The platform service and the tool registry must be available. */
export const inject = ['tools', 'ncag']

/**
 * Register the NC-AG tools.
 * @param ctx - the owning context.
 */
export function apply(ctx: Context): void {
  for (const definition of createNcagTools(ctx)) ctx.tools.register(definition)
}
