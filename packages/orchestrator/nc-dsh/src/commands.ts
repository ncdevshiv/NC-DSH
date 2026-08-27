/**
 * `/nc-dsh` command: spawn the durable nc-dsh teammate.
 * @module @deepseek-ai/dsh-orchestrator-nc-dsh/commands
 */
import type { Context } from '@deepseek-ai/cordis'
import type { CommandInvocation, CommandResult } from '@deepseek-ai/dsh-commands'

export function registerNcDshCommand(ctx: Context): () => void {
  return ctx.commands.register({
    name: 'nc-dsh',
    description: 'spawn the nc-dsh orchestrator teammate that owns the build graph',
    handler: async (invocation: CommandInvocation): Promise<CommandResult> => {
      const agent = invocation.agent
      // Validate Lead membership: only the Team Lead may spawn teammates.
      try {
        const membership = ctx.agentTeams.membership(agent)
        if (membership.role !== 'lead') {
          return { kind: 'error', text: 'Only the Team Lead may spawn nc-dsh. This agent is not the Lead.' }
        }
      } catch (error: unknown) {
        return {
          kind: 'error',
          text: `Cannot spawn nc-dsh: ${error instanceof Error ? error.message : String(error)}`,
        }
      }

      // Idempotency: if nc-dsh already exists, surface it.
      try {
        const members = ctx.agentTeams.listMembers(agent)
        const existing = members.find(member => member.name === 'nc-dsh')
        if (existing !== undefined) {
          return { kind: 'success', text: `nc-dsh already active: ${existing.name} (${existing.status})` }
        }
      } catch {}

      const orchestrator = ctx.get('orchestratorNcDsh')
      if (orchestrator === undefined) {
        return { kind: 'error', text: 'orchestrator-nc-dsh service not available' }
      }

      try {
        await orchestrator.spawnTeammate(agent, invocation.signal)
        return { kind: 'success', text: 'nc-dsh spawned: owns build graph and write-scope arbitration' }
      } catch (error: unknown) {
        // Name collision surfaces diagnostics; caller can inspect maxMembers headroom.
        const message = error instanceof Error ? error.message : String(error)
        if (message.includes('nc-dsh') || message.includes('already exists') || message.includes('maxMembers')) {
          return { kind: 'error', text: `nc-dsh spawn failed (name burned or team full): ${message}` }
        }
        return { kind: 'error', text: `nc-dsh spawn failed: ${message}` }
      }
    },
  })
}
