/**
 * Recovery: retry classification, exponential backoff, circuit breaking,
 * checkpoints, restart reconciliation and stuck-agent detection.
 *
 * The rule every helper here enforces is the one that makes autonomous
 * execution safe: before repeating a write, determine whether the earlier
 * attempt could already have succeeded, and never repeat a non-idempotent
 * operation because an acknowledgement was lost.
 * @module @deepseek-ai/dsh-ncag-orchestrator/recovery
 */

import { newId } from '@deepseek-ai/dsh-ncag-core'
import type { AgentInstance, AgentStatus, Task } from '@deepseek-ai/dsh-ncag-core'
import type { Repositories } from '@deepseek-ai/dsh-ncag-store'

/** Whether an operation is safe to repeat after an unknown outcome. */
export type Idempotency = 'idempotent' | 'non-idempotent'

/** Backoff schedule for one retry loop. */
export interface BackoffConfig {
  readonly initialMs: number
  readonly multiplier: number
  readonly maxMs: number
  readonly maxAttempts: number
}

/** Default bounded retry schedule. */
export const DEFAULT_BACKOFF: BackoffConfig = { initialMs: 500, multiplier: 2, maxMs: 30_000, maxAttempts: 4 }

/**
 * The delay before one retry attempt.
 * @param attempt - zero-based attempt index.
 * @param config - the schedule.
 * @returns the delay in milliseconds.
 */
export function backoffDelay(attempt: number, config: BackoffConfig): number {
  const raw = config.initialMs * Math.pow(config.multiplier, attempt)
  return Math.min(config.maxMs, Math.round(raw))
}

/**
 * Sleep for one delay, resolving early only when the signal aborts.
 * @param ms - delay in milliseconds.
 * @param signal - optional cancellation.
 * @returns a promise settling when the delay elapses.
 */
export function sleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted === true) {
      reject(new Error('ncag: wait aborted'))
      return
    }
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort)
      resolve()
    }, ms)
    const onAbort = (): void => {
      clearTimeout(timer)
      reject(new Error('ncag: wait aborted'))
    }
    signal?.addEventListener('abort', onAbort, { once: true })
  })
}

/**
 * Run one operation under a bounded retry schedule.
 * @param operation - the operation to run; it receives the attempt index.
 * @param config - the retry schedule.
 * @param options - classification and cancellation.
 * @returns the operation result.
 * @throws the last error when every attempt fails.
 */
export async function withRetry<T>(
  operation: (attempt: number) => Promise<T>,
  config: BackoffConfig,
  options: {
    readonly idempotency: Idempotency
    readonly signal?: AbortSignal
    readonly beforeRetry?: (attempt: number, error: Error) => void
  },
): Promise<T> {
  let lastError: Error | undefined
  for (let attempt = 0; attempt < config.maxAttempts; attempt++) {
    try {
      return await operation(attempt)
    } catch (error: unknown) {
      lastError = error instanceof Error ? error : new Error(String(error))
      if (attempt === config.maxAttempts - 1) break
      if (options.idempotency === 'non-idempotent') {
        throw new Error(`ncag: refusing to retry a non-idempotent operation after unknown outcome: ${lastError.message}`)
      }
      options.beforeRetry?.(attempt + 1, lastError)
      await sleep(backoffDelay(attempt, config), options.signal)
    }
  }
  throw lastError ?? new Error('ncag: retry loop produced no result')
}

/** A circuit breaker that stops calling one provider after repeated failures. */
export interface BreakerConfig {
  readonly failureThreshold: number
  readonly openMs: number
}

/**
 * One provider circuit breaker.
 */
export class CircuitBreaker {
  private failures: number = 0
  private openedAtMs: number | null = null
  private readonly config: BreakerConfig

  /**
   * @param config - threshold and open duration.
   */
  constructor(config: BreakerConfig) {
    this.config = config
  }

  /**
   * Whether a call may proceed.
   * @param nowMs - current time in milliseconds.
   * @returns whether the breaker is closed (or half-open for one probe).
   */
  canCall(nowMs: number): boolean {
    if (this.openedAtMs === null) return true
    if (nowMs - this.openedAtMs >= this.config.openMs) {
      this.openedAtMs = null
      this.failures = 0
      return true
    }
    return false
  }

  /**
   * Record one failed call and open the breaker when the threshold is reached.
   * @param nowMs - current time in milliseconds.
   */
  fail(nowMs: number): void {
    this.failures += 1
    if (this.failures >= this.config.failureThreshold) this.openedAtMs = nowMs
  }

  /** Record one successful call, closing the breaker. */
  succeed(): void {
    this.failures = 0
    this.openedAtMs = null
  }

  /**
   * Read the breaker state.
   * @returns failures since the last success and whether it is open.
   */
  state(): { failures: number; open: boolean } {
    return { failures: this.failures, open: this.openedAtMs !== null }
  }
}

/** Evidence gathered about one possibly stuck agent. */
export interface StallEvidence {
  readonly agentId: string
  readonly kind: string
  readonly detail: string
}

/** Limits that decide when an agent is stuck rather than merely slow. */
export interface StallPolicy {
  /** Milliseconds without a heartbeat before an agent is considered unresponsive. */
  readonly heartbeatTimeoutMs: number
  /** Identical tool invocations before a loop is reported. */
  readonly repeatedToolThreshold: number
  /** Milliseconds without new activity and without progress evidence. */
  readonly silentTimeoutMs: number
}

/**
 * Detect whether one agent is stalled.
 * @param agent - the agent under observation.
 * @param lastProgressAt - when the agent last produced durable progress.
 * @param repeatedInvocations - the highest identical-invocation count recorded.
 * @param policy - the stall limits.
 * @param nowMs - current time in milliseconds.
 * @returns the evidence, or an empty list when the agent is progressing.
 */
export function detectStall(
  agent: AgentInstance,
  lastProgressAt: number,
  repeatedInvocations: number,
  policy: StallPolicy,
  nowMs: number,
): StallEvidence[] {
  const evidence: StallEvidence[] = []
  const lastHeartbeat = Date.parse(agent.lastHeartbeatAt)
  if (Number.isFinite(lastHeartbeat) && nowMs - lastHeartbeat > policy.heartbeatTimeoutMs) {
    evidence.push({ agentId: agent.id, kind: 'heartbeat-timeout', detail: `no heartbeat for ${nowMs - lastHeartbeat} ms` })
  }
  if (nowMs - lastProgressAt > policy.silentTimeoutMs) {
    evidence.push({ agentId: agent.id, kind: 'no-progress', detail: `no durable progress for ${nowMs - lastProgressAt} ms` })
  }
  if (repeatedInvocations >= policy.repeatedToolThreshold) {
    evidence.push({ agentId: agent.id, kind: 'repeated-tool', detail: `${repeatedInvocations} identical tool invocations` })
  }
  return evidence
}

/** State recorded at a checkpoint and restored after a restart. */
export interface CheckpointState {
  readonly agentStatuses: readonly { readonly agentId: string; readonly status: AgentStatus; readonly taskId: string | null }[]
  readonly taskStatuses: readonly { readonly taskId: string; readonly status: Task['status'] }[]
}

/**
 * Store one execution checkpoint.
 * @param repositories - store repositories.
 * @param projectId - project identifier.
 * @param reason - why the checkpoint was taken.
 * @param state - the captured state.
 * @returns the checkpoint identifier.
 */
export function writeCheckpoint(repositories: Repositories, projectId: string, reason: string, state: CheckpointState): string {
  const id = newId('checkpoint')
  repositories.checkpoints.save({
    id,
    projectId,
    at: new Date().toISOString(),
    reason,
    state: { agentStatuses: [...state.agentStatuses], taskStatuses: [...state.taskStatuses] },
  })
  return id
}
