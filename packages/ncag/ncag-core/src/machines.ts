/**
 * The agent and task lifecycle state machines. Every persisted status change
 * goes through {@link transitionAgent} or {@link transitionTask}, so an
 * invalid transition fails loudly instead of corrupting a durable record.
 * @module @deepseek-ai/dsh-ncag-core/machines
 */

import type { AgentStatus, TaskStatus } from './types.ts'

/** Kind of lifecycle machine a transition was attempted on. */
export type MachineKind = 'agent' | 'task'

/**
 * Raised when a lifecycle transition is not permitted.
 */
export class NcagStateError extends Error {
  /** The machine the invalid transition belongs to. */
  readonly machine: MachineKind
  /** The state the record was in. */
  readonly from: string
  /** The state the caller requested. */
  readonly to: string

  /**
   * @param machine - lifecycle machine kind.
   * @param from - current state.
   * @param to - requested state.
   */
  constructor(machine: MachineKind, from: string, to: string) {
    super(`ncag: ${machine} cannot move from ${from} to ${to}`)
    this.name = 'NcagStateError'
    this.machine = machine
    this.from = from
    this.to = to
  }
}
const AGENT_TRANSITIONS: Record<AgentStatus, readonly AgentStatus[]> = {
  CREATED: ['INITIALIZING', 'STOPPING', 'FAILED'],
  INITIALIZING: ['STARTING', 'STOPPING', 'FAILED'],
  STARTING: ['IDLE', 'WORKING', 'STOPPING', 'FAILED'],
  IDLE: ['ASSIGNED', 'REVIEWING', 'STOPPING', 'FAILED'],
  ASSIGNED: ['WORKING', 'WAITING', 'BLOCKED', 'HANDOFF_PENDING', 'STOPPING', 'FAILED'],
  WORKING: ['IDLE', 'WAITING', 'BLOCKED', 'HELP_REQUESTED', 'REVIEWING', 'HANDOFF_PENDING', 'STOPPING', 'FAILED'],
  WAITING: ['WORKING', 'BLOCKED', 'RECOVERING', 'STOPPING', 'FAILED'],
  BLOCKED: ['WORKING', 'HELP_REQUESTED', 'RECOVERING', 'STOPPING', 'FAILED'],
  HELP_REQUESTED: ['WORKING', 'BLOCKED', 'RECOVERING', 'STOPPING', 'FAILED'],
  REVIEWING: ['WORKING', 'IDLE', 'STOPPING', 'FAILED'],
  HANDOFF_PENDING: ['IDLE', 'WORKING', 'STOPPING', 'FAILED'],
  RECOVERING: ['WORKING', 'IDLE', 'STOPPING', 'FAILED'],
  STOPPING: ['STOPPED', 'FAILED'],
  STOPPED: [],
  FAILED: ['RECOVERING', 'STOPPING'],
}

const TASK_TRANSITIONS: Record<TaskStatus, readonly TaskStatus[]> = {
  CREATED: ['ANALYZING', 'CANCELLED'],
  ANALYZING: ['PLANNED', 'WAITING_APPROVAL', 'BLOCKED', 'CANCELLED'],
  PLANNED: ['WAITING_APPROVAL', 'READY', 'CANCELLED'],
  WAITING_APPROVAL: ['READY', 'REJECTED', 'BLOCKED', 'CANCELLED'],
  READY: ['ASSIGNED', 'BLOCKED', 'CANCELLED'],
  ASSIGNED: ['IN_PROGRESS', 'BLOCKED', 'FAILED', 'CANCELLED'],
  IN_PROGRESS: ['REVIEW_PENDING', 'VERIFICATION_PENDING', 'BLOCKED', 'FAILED', 'CANCELLED'],
  BLOCKED: ['READY', 'ASSIGNED', 'IN_PROGRESS', 'FAILED', 'CANCELLED'],
  REVIEW_PENDING: ['IN_PROGRESS', 'VERIFICATION_PENDING', 'REJECTED', 'FAILED', 'CANCELLED'],
  VERIFICATION_PENDING: ['COMPLETED', 'IN_PROGRESS', 'REVIEW_PENDING', 'FAILED', 'CANCELLED'],
  COMPLETED: [],
  REJECTED: ['IN_PROGRESS', 'CANCELLED'],
  FAILED: ['READY', 'CANCELLED'],
  CANCELLED: [],
}

/**
 * Whether an agent may move between two states.
 * @param from - current agent state.
 * @param to - requested agent state.
 * @returns whether the transition is part of the machine.
 */
export function canTransitionAgent(from: AgentStatus, to: AgentStatus): boolean {
  return AGENT_TRANSITIONS[from].includes(to)
}

/**
 * Whether a task may move between two states.
 * @param from - current task state.
 * @param to - requested task state.
 * @returns whether the transition is part of the machine.
 */
export function canTransitionTask(from: TaskStatus, to: TaskStatus): boolean {
  return TASK_TRANSITIONS[from].includes(to)
}

/**
 * Validate one agent transition and return the requested state.
 * @param from - current agent state.
 * @param to - requested agent state.
 * @returns the requested state.
 * @throws NcagStateError when the transition is not permitted.
 */
export function transitionAgent(from: AgentStatus, to: AgentStatus): AgentStatus {
  if (from === to) return to
  if (!canTransitionAgent(from, to)) throw new NcagStateError('agent', from, to)
  return to
}

/**
 * Validate one task transition and return the requested state.
 * @param from - current task state.
 * @param to - requested task state.
 * @returns the requested state.
 * @throws NcagStateError when the transition is not permitted.
 */
export function transitionTask(from: TaskStatus, to: TaskStatus): TaskStatus {
  if (from === to) return to
  if (!canTransitionTask(from, to)) throw new NcagStateError('task', from, to)
  return to
}

/**
 * The full permitted destination list of one state, for dashboards and tests.
 * @param machine - lifecycle machine kind.
 * @param state - state to inspect.
 * @returns permitted destination states.
 */
export function allowedTransitions(machine: MachineKind, state: string): readonly string[] {
  if (machine === 'agent') return AGENT_TRANSITIONS[state as AgentStatus]
  return TASK_TRANSITIONS[state as TaskStatus]
}
