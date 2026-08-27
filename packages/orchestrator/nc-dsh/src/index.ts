import type { Context } from '@deepseek-ai/cordis'
import { Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type { Agent } from '@deepseek-ai/dsh-agent'
import type {} from '@deepseek-ai/dsh-experimental-agent-team'
import { ORCHESTRATOR_PROMPT } from './preset.ts'
import { registerNcDshCommand } from './commands.ts'

export const name = 'orchestrator-nc-dsh'
export const inject = ['agents', 'agentTeams', 'commands'] as const

export interface Config {
  teammateName?: string
  promptPrefix?: string
}

export const Config: z<Config> = z.object({
  teammateName: z.string().min(1).max(64).default('nc-dsh'),
  promptPrefix: z.string().default(''),
})

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
  ctx.effect(() => registerNcDshCommand(ctx), 'orchestrator-nc-dsh: /nc-dsh command')
}

export default OrchestratorService
export { ORCHESTRATOR_PROMPT } from './preset.ts'
export { NC_DSH_PRESET } from './preset.ts'
