/**
 * The decision center: structured disagreements, decision categories, and
 * the permanent record of every choice and every rejected recommendation.
 * @module @deepseek-ai/dsh-ncag-orchestrator/decisions
 */

import { newId } from '@deepseek-ai/dsh-ncag-core'
import type {
  DebateCase, DecisionAlternative, DecisionCategory, DecisionRecord, ProjectId, TaskId,
} from '@deepseek-ai/dsh-ncag-core'
import type { Repositories } from '@deepseek-ai/dsh-ncag-store'

/** The structured disagreement a caller supplies. */
export interface DebateInput {
  /** What the user asked for, restated accurately. */
  readonly requested: string
  /** What NC-AG believes should happen. */
  readonly belief: string
  /** The exact conflict. */
  readonly conflict: string
  /** The recommended alternative. */
  readonly recommendation: string
  /** Both approaches compared on the same axes. */
  readonly alternatives: readonly DecisionAlternative[]
  /** Evidence supporting the recommendation. */
  readonly evidence: readonly string[]
  /** The exact question put to the decision maker. */
  readonly question: string
}

/** A decision the orchestrator proposes. */
export interface ProposeDecisionInput {
  readonly projectId: ProjectId
  readonly taskId: TaskId | null
  readonly category: DecisionCategory
  /** Who proposes it: the orchestrator or an agent identity. */
  readonly author: string
  readonly problem: string
  /** Present for material decisions: the full debate case. */
  readonly debate: DebateInput | null
  /** The proposed choice. */
  readonly recommendation: string
  /** Messages or modules the decision affects. */
  readonly affectedModules?: readonly string[]
  readonly affectedTaskIds?: readonly TaskId[]
  readonly consequences?: readonly string[]
}

/**
 * Decision records. A material decision waits for an answer; an autonomous
 * decision is recorded immediately as accepted.
 */
export class DecisionCenter {
  private readonly repositories: Repositories

  /**
   * @param repositories - store repositories owning decision rows.
   */
  constructor(repositories: Repositories) {
    this.repositories = repositories
  }

  /**
   * Record one proposed decision.
   * @param input - what is being decided and why.
   * @returns the stored decision record.
   */
  propose(input: ProposeDecisionInput): DecisionRecord {
    const awaiting = input.category === 'material-architectural' || input.category === 'user-reserved'
    const now = new Date().toISOString()
    const record: DecisionRecord = {
      id: newId('decision'),
      projectId: input.projectId,
      taskId: input.taskId,
      category: input.category,
      author: input.author,
      problem: input.problem,
      debate: input.debate === null ? null : { ...input.debate },
      recommendation: input.recommendation,
      outcome: awaiting ? '' : input.recommendation,
      rationale: null,
      rejectedRecommendation: null,
      consequences: input.consequences ?? [],
      affectedModules: input.affectedModules ?? [],
      affectedTaskIds: input.affectedTaskIds ?? [],
      status: awaiting ? 'AWAITING_USER' : 'ACCEPTED',
      createdAt: now,
      decidedAt: awaiting ? null : now,
      reviewHistory: [`${now} proposed by ${input.author}`],
    }
    this.repositories.decisions.save(record)
    return record
  }

  /**
   * Record the final answer to a decision.
   * @param decisionId - decision identifier.
   * @param outcome - the choice actually taken.
   * @param rationale - the decision maker's stated reason, when supplied.
   * @param decidedBy - who decided.
   * @returns the updated record.
   * @throws Error when the decision does not exist.
   */
  decide(decisionId: string, outcome: string, rationale: string | null, decidedBy: string): DecisionRecord {
    const current = this.repositories.decisions.get(decisionId)
    if (current === null) throw new Error(`ncag: decision ${decisionId} does not exist`)
    const overridden = current.recommendation !== outcome
    const now = new Date().toISOString()
    const updated: DecisionRecord = {
      ...current,
      outcome,
      rationale,
      rejectedRecommendation: overridden ? current.recommendation : null,
      status: overridden ? 'OVERRIDDEN' : 'ACCEPTED',
      decidedAt: now,
      reviewHistory: [...current.reviewHistory, `${now} decided by ${decidedBy}: ${outcome}`],
    }
    this.repositories.decisions.save(updated)
    return updated
  }

  /**
   * Read decisions waiting for an answer.
   * @returns the pending decisions.
   */
  pending(): DecisionRecord[] {
    return this.repositories.decisions.pending()
  }

  /**
   * Read one project decision history.
   * @param projectId - project identifier.
   * @returns the decisions in creation order.
   */
  history(projectId: string): DecisionRecord[] {
    return this.repositories.decisions.listForProject(projectId)
  }

  /**
   * Assemble a debate case, refusing one that omits a required part.
   * @param input - the raw disagreement.
   * @returns the complete debate case.
   * @throws Error when a required part is empty.
   */
  static debate(input: DebateInput): DebateCase {
    const required: readonly [keyof DebateInput, string][] = [
      ['requested', 'what was requested'],
      ['belief', 'what NC-AG believes'],
      ['conflict', 'the conflict'],
      ['recommendation', 'the recommendation'],
      ['question', 'the decision question'],
    ]
    for (const [field, label] of required) {
      const value = input[field]
      if (typeof value !== 'string' || value.trim().length === 0) {
        throw new Error(`ncag: a debate case must state ${label}`)
      }
    }
    if (input.alternatives.length < 2) {
      throw new Error('ncag: a debate case must compare at least two alternatives')
    }
    if (input.evidence.length === 0) {
      throw new Error('ncag: a debate case must present evidence')
    }
    return {
      requested: input.requested,
      belief: input.belief,
      conflict: input.conflict,
      recommendation: input.recommendation,
      alternatives: [...input.alternatives],
      evidence: [...input.evidence],
      question: input.question,
    }
  }
}
