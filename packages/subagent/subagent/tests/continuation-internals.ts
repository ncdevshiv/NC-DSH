/** Package-private continuation owners used to place deterministic lifecycle races. */

import type { Context } from '@deepseek-ai/cordis'
import type { SessionId } from '@deepseek-ai/dsh-session'
import type { Activation } from '../src/activation.ts'
import type SubagentManager from '../src/manager.ts'

/** Return the service's bound subagent manager. */
export function continuationManager(ctx: Context): SubagentManager {
  const manager = ctx.subagents['manager']
  if (manager === undefined) throw new Error('expected a bound continuation manager')
  return manager
}

/** Return the manager's sole process-local Activation owner. */
export function continuationActivations(ctx: Context): { resident: Map<SessionId, Activation> } {
  return {
    resident: continuationManager(ctx)['resident'] as Map<SessionId, Activation>,
  }
}

/** Remove only the registry entry, leaving its Agent live for collision coverage. */
export function dropContinuationActivation(ctx: Context, childId: SessionId): void {
  continuationActivations(ctx).resident.delete(childId)
}
