import type { Context } from '@deepseek-ai/cordis'
import { Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type { Agent } from '@deepseek-ai/dsh-agent'
import type {} from '@deepseek-ai/dsh-experimental-agent-team'

export const name = 'orchestrator-nc-dsh'
export const inject = ['agents', 'agentTeams'] as const

export interface Config {
  teammateName?: string
  promptPrefix?: string
}

export const Config: z<Config> = z.object({
  teammateName: z.string().min(1).max(64).default('nc-dsh'),
  promptPrefix: z.string().default(''),
})

const ORCHESTRATOR_PROMPT =
  'You are nc-dsh, the dedicated build orchestrator teammate for this harness.\n\n' +
  'Own the Team task graph and write-scope arbitration. Keep the harness live while it upgrades itself:\n\n' +
  '- Partition work into disjoint scopes. Record expected writeScopes on every shared task and use blockedBy when work must be ordered.\n' +
  '- Use send_message for quiet information that must not start an idle teammate. Use followup_task when the target should run another turn.\n' +
  '- Claim with the current revision, perform the work, then complete. Re-list after wakeup or timeout.\n' +
  '- During a generation cutover, quiesce the old generation, shadow-build and health-probe the new binary, then atomically promote.\n' +
  '- Reap stale in_progress tasks where the owner went idle or interrupted.\n' +
  'You share the working directory with every member. Edits are immediately visible. Prefer read/edit/write and rebase on FS_STALE_VERSION.'

declare module '@deepseek-ai/cordis' {
  interface Context {
    orchestratorNcDsh: OrchestratorService
  }
}

export class OrchestratorService extends Service {
  static Config: z<Config> = Config

  private readonly teammateName: string
  private readonly promptPrefix: string

  constructor(ctx: Context, config: Config) {
    super(ctx, 'orchestratorNcDsh')
    this.teammateName = config.teammateName ?? 'nc-dsh'
    this.promptPrefix = config.promptPrefix ?? ''
  }

  get name_(): string {
    return this.teammateName
  }

  async spawnTeammate(lead: Agent, signal?: AbortSignal): Promise<unknown> {
    const prompt = this.promptPrefix.length > 0 ? `${this.promptPrefix}\n\n${ORCHESTRATOR_PROMPT}` : ORCHESTRATOR_PROMPT
    const abortSignal = signal ?? new AbortController().signal
    return await this.ctx.agentTeams.spawnTeammate(lead, {
      name: this.teammateName,
      description: 'nc-dsh orchestrator — owns task graph, write-scope arbitration, and generation cutover',
      prompt: [{ type: 'text', text: prompt }],
      context: 'fresh' as const,
      provider: 'spawn',
      signal: abortSignal,
    })
  }

  findTeammate(lead: Agent): unknown {
    try {
      const members = this.ctx.agentTeams.listMembers(lead)
      return members.find((member: { name: string }) => member.name === this.teammateName)
    } catch {
      return undefined
    }
  }
}

export function apply(ctx: Context, config: Config): void {
  const service = new OrchestratorService(ctx, config)
  ctx.provide('orchestratorNcDsh', service)
}

export default OrchestratorService
