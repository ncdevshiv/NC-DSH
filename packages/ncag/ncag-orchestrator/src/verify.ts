/**
 * The verifier: it executes the checks each acceptance criterion names and
 * records the exact outcome as evidence. A criterion whose verification is a
 * manual review is answered from the recorded reviewer findings, so an absent
 * review fails the criterion instead of passing it by default. Completion is
 * computed from the recorded evidence, never from an agent report.
 * @module @deepseek-ai/dsh-ncag-orchestrator/verify
 */

import { execFileSync } from 'node:child_process'
import { resolveShell } from '@deepseek-ai/dsh-ncag-runtime'
import type { AcceptanceCriterion, AgentId, Project, ProjectId, TaskId, VerificationEvidence, VerificationStatus } from '@deepseek-ai/dsh-ncag-core'
import type { Repositories } from '@deepseek-ai/dsh-ncag-store'

/** Maximum characters of one command output kept as evidence. */
export const MAX_EVIDENCE_OUTPUT = 8000

/** Result of one verification run. */
export interface VerificationResult {
  readonly evidence: VerificationEvidence
  readonly status: VerificationStatus
}

/** One command outcome. */
interface CommandOutcome {
  readonly status: VerificationStatus
  readonly exitCode: number | null
  readonly output: string
}

/** Run one command in the agent workspace and capture its exact outcome. */
function runCommand(command: string, workspacePath: string): CommandOutcome {
  try {
    const shell = resolveShell()
    const output = execFileSync(shell, ['-NoProfile', '-NonInteractive', '-Command', command], {
      cwd: workspacePath,
      encoding: 'utf8',
      timeout: 600_000,
      stdio: ['ignore', 'pipe', 'pipe'],
      maxBuffer: 16 * 1024 * 1024,
    })
    return { status: 'PASSED', exitCode: 0, output: output.slice(0, MAX_EVIDENCE_OUTPUT) }
  } catch (error: unknown) {
    const failure = error as { stdout?: string; stderr?: string; status?: number; message?: string }
    /* v8 ignore next -- a spawn or exit failure always carries a message; the final fallback is unreachable defense. */
    const combined = `${failure.stdout ?? ''}${failure.stderr ?? ''}` || failure.message || 'command failed'
    return { status: 'FAILED', exitCode: typeof failure.status === 'number' ? failure.status : null, output: combined.slice(0, MAX_EVIDENCE_OUTPUT) }
  }
}

/**
 * Execute one acceptance criterion and record its evidence.
 * @param repositories - store repositories owning evidence rows.
 * @param request - task identity, criterion, workspace and producing agent.
 * @returns the recorded result.
 */
export function verifyCriterion(
  repositories: Repositories,
  request: {
    readonly projectId: ProjectId
    readonly taskId: TaskId
    readonly criterion: AcceptanceCriterion
    readonly workspacePath: string
    readonly producedBy: AgentId | null
    readonly project: Project
  },
): VerificationResult {
  const configured = request.project.checks[request.criterion.verification]
  const command = request.criterion.command ?? configured
  // A manual review has no command to run: its evidence is the recorded
  // execution of an independent review plus the findings it raised. An absent
  // review is not an approval, and a CRITICAL or HIGH finding fails it.
  const reviewDerived = command === undefined && request.criterion.verification === 'manual-review'
  const runs = reviewDerived ? repositories.reviews.listForTask(request.taskId) : []
  const recorded = reviewDerived
    ? repositories.findings.listForTask(request.taskId).filter(finding => runs.some(run => run.reviewKind === finding.reviewKind))
    : []
  let outcome: CommandOutcome
  if (reviewDerived) {
    const blocking = recorded.filter(finding => finding.severity === 'CRITICAL' || finding.severity === 'HIGH')
    const passed = runs.length > 0 && blocking.length === 0
    outcome = {
      status: passed ? 'PASSED' : 'FAILED',
      exitCode: null,
      output: passed
        ? `manual review approved by ${recorded[0]?.reviewerAgentId ?? 'reviewer'}: ${recorded.length} finding(s), highest severity ${recorded[0]?.severity ?? 'NONE'}`
        : runs.length === 0
          ? `criterion ${request.criterion.id} requires an executed manual review, and no review was executed for task ${request.taskId}`
          : `manual review rejected: ${blocking.map(finding => `${finding.severity} ${finding.artifact}: ${finding.description}`).join('; ')}`,
    }
  } else {
    outcome = command === undefined
      ? {
        status: 'FAILED',
        exitCode: null,
        output: `criterion ${request.criterion.id} declares no command, so a ${request.criterion.verification} check cannot be executed`,
      }
      : runCommand(command, request.workspacePath)
  }
  const evidence: VerificationEvidence = {
    taskId: request.taskId,
    criterionId: request.criterion.id,
    method: request.criterion.verification,
    status: outcome.status,
    command: command ?? null,
    exitCode: outcome.exitCode,
    output: outcome.output,
    recordedAt: new Date().toISOString(),
    producedBy: request.producedBy,
  }
  repositories.evidence.append(request.projectId, evidence)
  return { evidence, status: outcome.status }
}

/**
 * Execute every criterion of one task.
 * @param repositories - store repositories owning evidence rows.
 * @param request - task identity, criteria, workspace and producing agent.
 * @returns the recorded results in criterion order.
 */
export function verifyTask(
  repositories: Repositories,
  request: {
    readonly projectId: ProjectId
    readonly taskId: TaskId
    readonly criteria: readonly AcceptanceCriterion[]
    readonly workspacePath: string
    readonly producedBy: AgentId | null
    readonly project: Project
  },
): VerificationResult[] {
  const results: VerificationResult[] = []
  for (const criterion of request.criteria) {
    results.push(verifyCriterion(repositories, {
      projectId: request.projectId,
      taskId: request.taskId,
      criterion,
      workspacePath: request.workspacePath,
      producedBy: request.producedBy,
      project: request.project,
    }))
  }
  return results
}

/**
 * Criteria of one task that still have no passing evidence.
 * @param repositories - store repositories owning evidence rows.
 * @param taskId - task identifier.
 * @param criteria - the criteria that must pass.
 * @returns the criterion identifiers without passing evidence; empty means verified.
 */
export function unverifiedCriteria(
  repositories: Repositories,
  taskId: string,
  criteria: readonly AcceptanceCriterion[],
): string[] {
  const recorded = repositories.evidence.listForTask(taskId)
  return criteria
    .filter(criterion => !recorded.some(entry => entry.criterionId === criterion.id && entry.status === 'PASSED'))
    .map(criterion => criterion.id)
}
