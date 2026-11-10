/**
 * Composition-owned table of the delegation tool definitions one composition
 * can show a model.
 * @module @deepseek-ai/dsh-tool-subagent/delegation-tool-table
 */

import type { Context } from '@deepseek-ai/cordis'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'

declare module '@deepseek-ai/cordis' {
  interface Context {
    /** The composition-owned table of the loaded delegation tool definitions. */
    toolSubagentDelegationTools: DelegationToolTable
  }
}

/** Service name of the composition-owned delegation-tool table. */
export const DELEGATION_TOOL_TABLE = 'toolSubagentDelegationTools'

/**
 * The delegation tool definitions every loaded instance of this package
 * contributes to one composition.
 *
 * The table holds one entry per loaded instance, so the model-visible guidance
 * can name every delegation tool an agent can see in one sentence. It is owned
 * by the composition, not by the module, so no table outlives the runtime that
 * filled it.
 */
export class DelegationToolTable {
  private readonly definitions = new Set<ToolDefinition>()

  /**
   * Contribute one delegation tool definition to the table.
   * @param definition - the definition a loaded instance registered.
   * @returns the disposer that withdraws the definition.
   */
  register(definition: ToolDefinition): () => void {
    this.definitions.add(definition)
    return () => {
      this.definitions.delete(definition)
    }
  }

  /**
   * Read the contributed definitions.
   * @returns the definitions in contribution order.
   */
  list(): readonly ToolDefinition[] {
    return [...this.definitions]
  }
}

/**
 * Resolve the composition-owned delegation-tool table.
 *
 * The first instance of this package in a composition publishes the table in
 * the root scope, which owns it for the whole composition: every later
 * instance, including one mounted in an Agent scope, reads the same instance
 * through the global service store. The root scope withdraws the service when
 * it unloads.
 * @param ctx - the context of the loaded instance.
 * @returns the one table this composition shares.
 */
export function resolveDelegationToolTable(ctx: Context): DelegationToolTable {
  const existing = ctx.get(DELEGATION_TOOL_TABLE)
  if (existing !== undefined) return existing
  const table = new DelegationToolTable()
  ctx.root.provide(DELEGATION_TOOL_TABLE, table)
  return table
}
