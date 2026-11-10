/**
 * NC-AG durable record vocabulary: projects, requests, requirements, tasks,
 * agents, messages, decisions, findings, leases, change sets, usage and audit
 * records, plus the enums their state machines move between.
 *
 * These are the only values the platform persists and exchanges between its
 * modules; runtime behavior lives in the modules that own each record lifecycle.
 * @module @deepseek-ai/dsh-ncag-core/types
 */
/** States from which an agent still accepts work or observation. */
export const LIVE_AGENT_STATUSES = [
    'CREATED', 'INITIALIZING', 'STARTING', 'IDLE', 'ASSIGNED', 'WORKING', 'WAITING',
    'BLOCKED', 'HELP_REQUESTED', 'REVIEWING', 'HANDOFF_PENDING', 'RECOVERING', 'STOPPING',
];
/** Task states in which the scheduler may consider work done. */
export const TERMINAL_TASK_STATUSES = ['COMPLETED', 'REJECTED', 'FAILED', 'CANCELLED'];
//# sourceMappingURL=types.js.map