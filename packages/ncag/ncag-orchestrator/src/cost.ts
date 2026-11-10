/**
 * Cost and resource governance: per-agent, per-task and per-project usage
 * accounting, configured ceilings, and the pause-and-ask policy that fires
 * before a hard budget is crossed.
 * @module @deepseek-ai/dsh-ncag-orchestrator/cost
 */

import { addUsage } from '@deepseek-ai/dsh-ncag-store'
import type { Repositories, UsageRecord } from '@deepseek-ai/dsh-ncag-store'
import type { AgentId, ProjectId, TaskId, UsageTotals } from '@deepseek-ai/dsh-ncag-core'
import { ModelRouter } from './router.ts'
import type { ResolvedRoute } from './router.ts'

/** One recorded model call, before pricing. */
export interface CallUsage {
  readonly projectId: ProjectId
  readonly taskId: TaskId | null
  readonly agentId: AgentId | null
  readonly route: ResolvedRoute
  readonly inputTokens: number
  readonly outputTokens: number
  readonly cachedTokens: number
  readonly purpose: string
}

/** A budget check the orchestrator must honour before dispatching more work. */
export interface BudgetDecision {
  /** Whether the operation may continue. */
  readonly allowed: boolean
  /** Current spend against the ceiling. */
  readonly spentUsd: number
  /** Configured ceiling: the project autonomy ceiling, or the explicit override. */
  readonly ceilingUsd: number
  /** Reason reported to the orchestrator when the operation is refused. */
  readonly reason: string | null
}

/** Zero totals used when no usage has accumulated yet. */
const EMPTY: UsageTotals = { inputTokens: 0, outputTokens: 0, cachedTokens: 0, costUsd: 0, calls: 0 }

/**
 * Usage ledger with budget enforcement.
 */
export class CostManager {
  private readonly repositories: Repositories
  private readonly agentUsage: Map<string, UsageTotals>
  private readonly taskUsage: Map<string, UsageTotals>

  /**
   * @param repositories - store repositories owning the usage ledger.
   */
  constructor(repositories: Repositories) {
    this.repositories = repositories
    this.agentUsage = new Map()
    this.taskUsage = new Map()
  }

  /**
   * Record one completed model call.
   *
   * Token counts are always recorded; USD is recorded only when the route is
   * priced, so an unpriced run still leaves a complete usage trail.
   * @param usage - the call to record.
   * @returns the cost in US dollars, or null when the route is unpriced.
   */
  record(usage: CallUsage): number | null {
    const costUsd = ModelRouter.price(usage.route, usage.inputTokens, usage.cachedTokens, usage.outputTokens)
    const record: UsageRecord = {
      at: new Date().toISOString(),
      projectId: usage.projectId,
      taskId: usage.taskId,
      agentId: usage.agentId,
      provider: usage.route.provider,
      model: usage.route.model,
      inputTokens: usage.inputTokens,
      outputTokens: usage.outputTokens,
      cachedTokens: usage.cachedTokens,
      costUsd: costUsd ?? 0,
      purpose: usage.purpose,
    }
    this.repositories.usage.append(record)
    const totals: UsageTotals = {
      inputTokens: usage.inputTokens,
      outputTokens: usage.outputTokens,
      cachedTokens: usage.cachedTokens,
      costUsd: costUsd ?? 0,
      calls: 1,
    }
    if (usage.agentId !== null) {
      this.agentUsage.set(
        usage.agentId,
        addUsage(this.agentUsage.get(usage.agentId) ?? EMPTY, totals),
      )
    }
    if (usage.taskId !== null) {
      this.taskUsage.set(
        usage.taskId,
        addUsage(this.taskUsage.get(usage.taskId) ?? EMPTY, totals),
      )
    }
    return costUsd
  }

  /**
   * Check a project against its ceiling before another operation.
   * A project without a ceiling is unlimited.
   * @param projectId - project identifier.
   * @param ceilingUsd - configured ceiling, or null for unlimited.
   * @returns whether work may continue.
   */
  budget(projectId: string, ceilingUsd: number | null): BudgetDecision {
    if (ceilingUsd === null) {
      return { allowed: true, spentUsd: 0, ceilingUsd: Number.POSITIVE_INFINITY, reason: null }
    }
    const spentUsd = this.repositories.usage.totals(projectId).costUsd
    if (spentUsd >= ceilingUsd) {
      return {
        allowed: false,
        spentUsd,
        ceilingUsd,
        reason: `project spend ${spentUsd.toFixed(4)} USD reached the configured ceiling of ${ceilingUsd.toFixed(4)} USD`,
      }
    }
    return { allowed: true, spentUsd, ceilingUsd, reason: null }
  }

  /**
   * Read the accumulated usage of one project.
   * @param projectId - project identifier.
   * @returns the totals.
   */
  projectTotals(projectId: string): UsageTotals {
    return this.repositories.usage.totals(projectId)
  }

  /**
   * Read the accumulated usage of one agent.
   * @param agentId - agent identifier.
   * @returns the totals.
   */
  agentTotals(agentId: string): UsageTotals {
    return this.agentUsage.get(agentId) ?? { inputTokens: 0, outputTokens: 0, cachedTokens: 0, costUsd: 0, calls: 0 }
  }

  /**
   * Read the accumulated usage of one task.
   * @param taskId - task identifier.
   * @returns the totals.
   */
  taskTotals(taskId: string): UsageTotals {
    return this.taskUsage.get(taskId) ?? { inputTokens: 0, outputTokens: 0, cachedTokens: 0, costUsd: 0, calls: 0 }
  }
}
