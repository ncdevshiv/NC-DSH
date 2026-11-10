/**
 * The task DAG: dependency validation, cycle detection, readiness computation,
 * priority ordering and critical-path analysis.
 *
 * A graph is accepted only when every dependency names an existing task and no
 * cycle exists, so the scheduler can never deadlock on unsatisfiable work.
 * @module @deepseek-ai/dsh-ncag-orchestrator/dag
 */

import { TERMINAL_TASK_STATUSES } from '@deepseek-ai/dsh-ncag-core'
import type { Task, TaskId, TaskStatus } from '@deepseek-ai/dsh-ncag-core'

/** One validation failure of a candidate graph. */
export interface GraphProblem {
  readonly taskId: string
  readonly problem: string
}

/** Project priority rank used by the scheduler; lower is earlier. */
const PRIORITY_RANK: Record<Task['priority'], number> = { P0: 0, P1: 1, P2: 2, P3: 3 }

/**
 * Validate a task graph.
 * @param tasks - the complete set of tasks of one project.
 * @returns every problem found; an empty list means the graph is executable.
 */
export function validateGraph(tasks: readonly Task[]): GraphProblem[] {
  const problems: GraphProblem[] = []
  const known = new Set<string>(tasks.map(task => task.id as string))
  for (const task of tasks) {
    for (const dependency of task.dependencyIds) {
      if (!known.has(dependency)) {
        problems.push({ taskId: task.id, problem: `dependency ${dependency} does not exist` })
      }
      if (dependency === task.id) {
        problems.push({ taskId: task.id, problem: 'a task cannot depend on itself' })
      }
    }
  }
  for (const cycle of findCycles(tasks)) {
    /* v8 ignore next -- findCycles always returns the repeated node, so a cycle is never empty. */
    problems.push({ taskId: cycle[0] ?? '', problem: `dependency cycle among ${cycle.length - 1} tasks` })
  }
  return problems
}

/**
 * Find dependency cycles with a depth-first search.
 * @param tasks - the task set.
 * @returns one representative cycle per strongly connected region.
 */
export function findCycles(tasks: readonly Task[]): string[][] {
  const edges = new Map<string, readonly string[]>()
  for (const task of tasks) edges.set(task.id, task.dependencyIds)
  const cycles: string[][] = []
  const visiting = new Set<string>()
  const visited = new Set<string>()
  const stack: string[] = []
  const walk = (node: string): void => {
    if (visited.has(node)) return
    if (visiting.has(node)) {
      const start = stack.indexOf(node)
      /* v8 ignore next -- a node marked visiting is on the stack, so indexOf always finds it. */
      if (start >= 0) cycles.push([...stack.slice(start), node])
      return
    }
    visiting.add(node)
    stack.push(node)
    /* v8 ignore next -- the walk only reaches task nodes, so the edge list is always present. */
    for (const dependency of edges.get(node) ?? []) {
      if (edges.has(dependency)) walk(dependency)
    }
    stack.pop()
    visiting.delete(node)
    visited.add(node)
  }
  for (const task of tasks) walk(task.id)
  return cycles
}

/**
 * Tasks whose dependencies are all terminal and that may therefore start.
 * @param tasks - the task set.
 * @returns the ready tasks, most urgent first.
 */
export function readyTasks(tasks: readonly Task[]): Task[] {
  const status = new Map<string, TaskStatus>()
  for (const task of tasks) status.set(task.id, task.status)
  return tasks
    .filter(task => (task.status === 'PLANNED' || task.status === 'READY') && task.assignedAgentId === null)
    .filter(task => task.dependencyIds.every((dependency) => {
      const dependencyStatus = status.get(dependency)
      return dependencyStatus !== undefined && TERMINAL_TASK_STATUSES.includes(dependencyStatus)
    }))
    .sort(compareTasks)
}

/**
 * Order two tasks by scheduling priority: user priority, deadline, then creation.
 * @param a - first task.
 * @param b - second task.
 * @returns the comparator result.
 */
export function compareTasks(a: Task, b: Task): number {
  const byPriority = PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority]
  if (byPriority !== 0) return byPriority
  const aDeadline = a.deadlineMs ?? Number.MAX_SAFE_INTEGER
  const bDeadline = b.deadlineMs ?? Number.MAX_SAFE_INTEGER
  if (aDeadline !== bDeadline) return aDeadline - bDeadline
  return a.createdAt.localeCompare(b.createdAt)
}

/**
 * Tasks on the critical path, computed as the longest dependency chain.
 * @param tasks - the task set.
 * @returns the tasks that determine the project duration, earliest first.
 */
export function criticalPath(tasks: readonly Task[]): TaskId[] {
  const byId = new Map<string, Task>()
  for (const task of tasks) byId.set(task.id, task)
  const depthOf = new Map<string, number>()
  const longest = (taskId: string, seen: Set<string>): number => {
    const cached = depthOf.get(taskId)
    if (cached !== undefined) return cached
    if (seen.has(taskId)) return 0
    seen.add(taskId)
    const task = byId.get(taskId)
    if (task === undefined) return 0
    let best = 0
    for (const dependency of task.dependencyIds) {
      best = Math.max(best, longest(dependency, seen))
    }
    depthOf.set(taskId, best + 1)
    return best + 1
  }
  let deepest = 0
  for (const task of tasks) deepest = Math.max(deepest, longest(task.id, new Set()))
  // Every task was visited above, so each depth lookup is defined, and the
  // reconstruction walks strictly decreasing depths, so it cannot revisit one.
  /* v8 ignore start -- the reconstruction is unreachable for a validated graph. */
  let cursor = tasks.find(task => (depthOf.get(task.id as string) ?? 0) === deepest)
  const path: TaskId[] = []
  while (cursor !== undefined) {
    if (path.includes(cursor.id)) break
    path.push(cursor.id)
    const currentDepth = depthOf.get(cursor.id) ?? 0
    const holders = cursor.dependencyIds
    cursor = tasks.find(task => holders.includes(task.id) && (depthOf.get(task.id as string) ?? 0) === currentDepth - 1)
  }
  /* v8 ignore stop */
  return path.reverse()
}

/**
 * Whether one task may move to the given status, considering its dependencies.
 * @param task - the task to move.
 * @param tasks - the complete task set.
 * @param to - requested status.
 * @returns null when the move is allowed, otherwise the blocking reason.
 */
export function blockingReason(task: Task, tasks: readonly Task[], to: TaskStatus): string | null {
  if (to !== 'READY' && to !== 'ASSIGNED' && to !== 'IN_PROGRESS') return null
  const byId = new Map<string, Task>()
  for (const candidate of tasks) byId.set(candidate.id, candidate)
  const open = task.dependencyIds.filter((dependency) => {
    const candidate = byId.get(dependency)
    return candidate === undefined || !TERMINAL_TASK_STATUSES.includes(candidate.status)
  })
  if (open.length > 0) return `dependencies still open: ${open.join(', ')}`
  return null
}
