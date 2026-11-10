/**
 * Independent review and acceptance verification.
 *
 * A reviewer always inspects the recorded evidence itself: the changed files,
 * the tool executions and the verification results. When a delegation route is
 * configured, a model reviewer adds a second opinion grounded in that same
 * evidence; its findings are advisory and every one must cite the artifact it
 * refers to, so a fabricated finding cannot pass as evidence.
 * @module @deepseek-ai/dsh-ncag-orchestrator/review
 */

import { newId } from '@deepseek-ai/dsh-ncag-core'
import { adoptId } from '@deepseek-ai/dsh-ncag-core'
import type { FindingSeverity, ReviewFinding, ReviewKind, Task } from '@deepseek-ai/dsh-ncag-core'
import type { DelegationPort } from './delegate.ts'

/** Everything a review sees about one finished task. */
export interface ReviewInput {
  /** The task under review. */
  readonly task: Task
  /** The agent completion report. */
  readonly report: string
  /** Repository-relative paths the agent changed. */
  readonly changedFiles: readonly string[]
  /** Tool executions recorded for the task, newest first. */
  readonly executions: readonly { readonly tool: string; readonly ok: boolean; readonly output: string }[]
  /** Verification results recorded for the task. */
  readonly verifications: readonly { readonly criterionId: string; readonly status: string; readonly output: string }[]
  /** Reviewer identity producing the findings. */
  readonly reviewerAgentId: string
  /** Review kind being performed. */
  readonly kind: ReviewKind
}

/** Outcome of one review. */
export interface ReviewOutcome {
  /** Findings raised, ordered by severity. */
  readonly findings: readonly ReviewFinding[]
  /** Whether the review rejects the task. */
  readonly rejected: boolean
  /** One-line summary recorded in the audit trail. */
  readonly summary: string
}

/** Severity ordering, most severe first. */
const SEVERITY_ORDER: readonly FindingSeverity[] = ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW', 'INFORMATIONAL']

/** Build one finding record. */
function finding(
  input: ReviewInput,
  severity: FindingSeverity,
  description: string,
  evidence: string,
  artifact: string,
  impact: string,
  recommendation: string,
): ReviewFinding {
  return {
    id: newId('finding'),
    taskId: input.task.id,
    projectId: input.task.projectId,
    reviewerAgentId: adoptId('agent', input.reviewerAgentId),
    reviewKind: input.kind,
    severity,
    description,
    evidence,
    artifact,
    impact,
    recommendation,
    verificationMethod: 're-run the affected check and observe its output',
    status: 'OPEN',
    createdAt: new Date().toISOString(),
    resolvedAt: null,
  }
}

/**
 * Review one task against its recorded evidence.
 * @param input - everything the review may consider.
 * @param delegate - optional model reviewer adding a second opinion.
 * @returns the review outcome.
 */
export async function reviewTask(input: ReviewInput, delegate: DelegationPort): Promise<ReviewOutcome> {
  const findings: ReviewFinding[] = []
  // A consultation delivers a written answer, so a change set is not its
  // deliverable; the report is reviewed instead and recorded in the context.
  if (input.changedFiles.length === 0 && !input.task.consultation) {
    findings.push(finding(
      input, 'HIGH',
      'The task produced no changed files.',
      `change set is empty (${input.executions.length} tool executions recorded)`,
      'workspace',
      'Nothing was delivered, so no acceptance criterion can pass.',
      'Either implement the task or report it as blocked with the reason.',
    ))
  }
  const failedExecutions = input.executions.filter(execution => !execution.ok)
  for (const execution of failedExecutions) {
    findings.push(finding(
      input, 'MEDIUM',
      `The ${execution.tool} command failed during execution.`,
      execution.output.slice(0, 400),
      `tool:${execution.tool}`,
      'A failing command means the delivered behavior was not proven at that point.',
      'Re-run the command and record a passing result, or explain why the failure is expected.',
    ))
  }
  const failedVerifications = input.verifications.filter(verification => verification.status !== 'PASSED')
  for (const verification of failedVerifications) {
    findings.push(finding(
      input, 'HIGH',
      `Acceptance criterion ${verification.criterionId} is not verified.`,
      verification.output.slice(0, 400),
      `criterion:${verification.criterionId}`,
      'The task cannot complete while a required criterion has no passing evidence.',
      'Fix the failing behavior and re-run the verification.',
    ))
  }
  if (input.report.trim().length === 0) {
    findings.push(finding(
      input, 'MEDIUM',
      'The agent delivered no completion report.',
      'empty report',
      'report',
      'The delivered work cannot be attributed or summarized from the record.',
      'Produce a completion report naming the changes and the commands run.',
    ))
  }
  if (delegate.available && input.kind !== 'acceptance') {
    const answer = await delegate.ask({
      purpose: `review:${input.kind}`,
      system: [
        'You are an independent reviewer. You receive the recorded evidence of one finished task.',
        'Report only defects you can point at in the evidence. Every finding must name the artifact it refers to.',
        'Answer with one line per finding in the form: SEVERITY | artifact | description | recommended correction.',
        'Severities: CRITICAL, HIGH, MEDIUM, LOW, INFORMATIONAL. Answer NONE when the evidence shows no defect.',
      ].join('\n'),
      prompt: [
        `Task: ${input.task.title}`,
        `Description: ${input.task.description}`,
        `Agent report: ${input.report}`,
        `Changed files: ${input.changedFiles.join(', ') || '(none)'}`,
        `Executions: ${input.executions.map(execution => `${execution.tool} ok=${execution.ok} ${execution.output.slice(0, 200)}`).join(' /// ') || '(none)'}`,
        `Verifications: ${input.verifications.map(verification => `${verification.criterionId} ${verification.status}`).join(', ') || '(none)'}`,
      ].join('\n'),
      maxTokens: 1200,
      signal: new AbortController().signal,
    })
    for (const line of answer.split('\n')) {
      const parts = line.split('|').map(part => part.trim())
      if (parts.length < 3) continue
      const severity = parts[0] as FindingSeverity
      if (!SEVERITY_ORDER.includes(severity)) continue
      findings.push(finding(
        input, severity,
        /* v8 ignore next -- parts has at least three entries, so the description is present. */
        parts[2] ?? '',
        `model reviewer: ${line}`,
        parts[1] || 'unspecified',
        'Reported by the independent model reviewer.',
        parts[3] ?? 'Address the finding before completion.',
      ))
    }
  }
  findings.sort((a, b) => SEVERITY_ORDER.indexOf(a.severity) - SEVERITY_ORDER.indexOf(b.severity))
  const rejected = findings.some(item => item.severity === 'CRITICAL' || item.severity === 'HIGH')
  return {
    findings,
    rejected,
    /* v8 ignore next 2 -- the else branch implies at least one finding, so the highest severity always exists. */
    summary: findings.length === 0
      ? `${input.kind} found no defect`
      : `${input.kind} raised ${findings.length} finding(s), highest severity ${findings[0]?.severity ?? 'UNKNOWN'}`,
  }
}
