/**
 * Wire types of the generated `ncag` Remote namespace: the platform snapshot,
 * the timeline, decisions, profiles, and the operator controls.
 *
 * Every record here is declared locally and structurally: the browser face must
 * not pull a Host package's source into its program, and the generated Remote
 * projection names exactly these types. A Host record satisfies them
 * structurally because every declared field is a subset of what the
 * orchestrator publishes.
 * @module @deepseek-ai/dsh-api-ncag-controller/types
 */

/** Model capability tier one route serves. */
export type NcagModelTier = 'reasoning' | 'standard' | 'fast'

/** Project lifecycle state, as the platform spells it. */
export type NcagProjectStatus = string

/** Agent lifecycle state, as the platform spells it. */
export type NcagAgentStatus = string

/** Task lifecycle state, as the platform spells it. */
export type NcagTaskStatus = string

/** One accumulated token and cost record. */
export interface NcagUsage {
  readonly inputTokens: number
  readonly outputTokens: number
  readonly costUsd: number
  readonly calls: number
}

/** One model route an agent runs on. */
export interface NcagModelRoute {
  readonly provider: string
  readonly model: string
}

/** One project row the monitor lists. */
export interface NcagProject {
  readonly id: string
  readonly name: string
  readonly description: string
  readonly status: NcagProjectStatus
  readonly repositoryPath: string
  readonly baseBranch: string
}

/** One task row of a project's graph. */
export interface NcagTask {
  readonly id: string
  readonly projectId: string
  readonly title: string
  readonly description: string
  readonly requiredCapability: string
  readonly status: NcagTaskStatus
  readonly priority: string
  readonly attempts: number
  readonly reviewCycles: number
  readonly statusReason: string | null
  readonly assignedAgentId: string | null
  readonly deadlineMs: number | null
}

/** One agent instance the platform spawned. */
export interface NcagAgent {
  readonly id: string
  readonly profileKey: string
  readonly projectId: string
  readonly taskId: string | null
  readonly status: NcagAgentStatus
  readonly model: NcagModelRoute
  readonly steps: number
  readonly usage: NcagUsage
  readonly failureCount: number
  readonly workspacePath: string | null
  readonly branch: string | null
  readonly lastHeartbeatAt: string
  readonly statusReason: string | null
}

/** The structured argument the platform puts to a human. */
export interface NcagDebateCase {
  readonly requested: string
  readonly belief: string
  readonly conflict: string
  readonly recommendation: string
  readonly evidence: readonly string[]
  readonly question: string
}

/** One recorded decision. */
export interface NcagDecision {
  readonly id: string
  readonly projectId: string
  readonly taskId: string | null
  readonly category: string
  readonly author: string
  readonly problem: string
  readonly debate: NcagDebateCase | null
  readonly recommendation: string
  readonly outcome: string
  readonly rationale: string | null
  readonly status: string
  readonly createdAt: string
  readonly decidedAt: string | null
}

/** One agent-to-agent or agent-to-user message, as a trace lists it. */
export interface NcagMessageView {
  readonly id: string
  readonly projectId: string
  readonly type: string
  readonly subject: string
  readonly content: string
  readonly deliveryStatus: string
}

/** One platform event as the monitor renders it; the structured payload stays on the host. */
export interface NcagEventView {
  /** Monotonic sequence in the event stream. */
  readonly sequence: number
  /** UTC timestamp. */
  readonly at: string
  /** Event kind, for example `task/transition`. */
  readonly kind: string
  readonly projectId: string | null
  readonly taskId: string | null
  readonly agentId: string | null
  /** One-line human summary. */
  readonly summary: string
}

/** One resolved model route with its effective price, when a price is configured. */
export interface NcagRouteView {
  readonly provider: string
  readonly model: string
  readonly tier: NcagModelTier
  /** USD per million input tokens, or null when the route is unpriced. */
  readonly inputPerMillion: number | null
  /** USD per million output tokens, or null when the route is unpriced. */
  readonly outputPerMillion: number | null
}

/** Audit-chain state the monitor header reports. */
export interface NcagAuditView {
  readonly records: number
  /** Sequence of the first broken link, or null while the chain verifies. */
  readonly brokenAt: number | null
}

/** One project's live state as the monitor lists and details it. */
export interface NcagProjectView {
  readonly project: NcagProject
  readonly requests: number
  readonly tasks: readonly NcagTask[]
  readonly agents: readonly NcagAgent[]
  readonly decisionsPending: number
  readonly leases: number
  readonly costUsd: number
}

/** Complete platform snapshot the monitor polls. */
export interface NcagSnapshotView {
  readonly generatedAt: string
  readonly projects: readonly NcagProjectView[]
  readonly usage: NcagUsage
  readonly events: readonly NcagEventView[]
  readonly audit: NcagAuditView
  readonly routes: readonly NcagRouteView[]
  /** Number of registered agent profiles. */
  readonly profiles: number
  /** Agents currently holding an execution slot. */
  readonly liveAgents: number
  /** Decisions awaiting a human answer across every project. */
  readonly pendingDecisions: number
}

/** One agent profile's summary, as the capability matrix renders it. */
export interface NcagProfileView {
  readonly id: string
  readonly key: string
  readonly role: string
  readonly capabilities: readonly string[]
  readonly modelTier: NcagModelTier
}

/** Audit entry of one correlation trace, without the hash-chain internals. */
export interface NcagTraceAuditView {
  readonly sequence: number
  readonly at: string
  readonly category: string
  readonly action: string
  readonly actor: string
  readonly projectId: string | null
  readonly taskId: string | null
  readonly agentId: string | null
}

/** One recorded tool execution inside a correlation trace. */
export interface NcagTraceToolCallView {
  readonly sequence: number
  readonly at: string
  readonly agentId: string
  readonly taskId: string | null
  readonly tool: string
  readonly ok: boolean
  readonly error: string | null
  readonly output: string
  readonly durationMs: number
}

/** One correlation's end-to-end trace. */
export interface NcagTraceView {
  readonly projectId: string
  readonly correlationId: string
  readonly audit: readonly NcagTraceAuditView[]
  readonly events: readonly NcagEventView[]
  readonly messages: readonly NcagMessageView[]
  readonly toolCalls: readonly NcagTraceToolCallView[]
  readonly tasks: readonly NcagTask[]
  readonly agents: readonly NcagAgent[]
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/** Target selection of one `ncag.snapshot` read. */
export interface NcagSnapshotRequest {
  /** Restrict the returned project entries to one project. */
  readonly projectId?: string
  /** Events included with the snapshot; omitted uses the orchestrator's own window. */
  readonly eventLimit?: number
}

/** Target of one `ncag.trace` read. */
export interface NcagTraceRequest {
  readonly projectId: string
  readonly correlationId: string
}

/** Target of one `ncag.recentEvents` read. */
export interface NcagEventsRequest {
  readonly projectId?: string
  /** Newest-last event count; clamped by the controller's configured ceiling. */
  readonly limit?: number
}

/** Target of one `ncag.decisions` read. */
export interface NcagDecisionsRequest {
  /** Project whose history to read; omitted reads every pending decision. */
  readonly projectId?: string
  /** Read only decisions awaiting a human answer. */
  readonly pendingOnly?: boolean
}

/** One project creation request. */
export interface NcagCreateProjectRequest {
  readonly name: string
  readonly description: string
  /** Absolute repository path; omitted anchors the project at the first workspace. */
  readonly repositoryPath?: string
  /** Project-wide instructions every agent receives. */
  readonly instructions?: string
}

/** One work request submitted to a project. */
export interface NcagSubmitRequest {
  readonly projectId: string
  readonly text: string
}

/** One project pause or resume. */
export interface NcagProjectControlRequest {
  readonly projectId: string
  readonly reason?: string
}

/** One task cancellation. */
export interface NcagTaskCancelRequest {
  readonly taskId: string
  readonly reason?: string
}

/** One task reassignment to a capability. */
export interface NcagTaskReassignRequest {
  readonly taskId: string
  /** Required capability the successor agent must hold. */
  readonly capability: string
  readonly reason?: string
}

/** One agent cancellation. */
export interface NcagAgentCancelRequest {
  readonly agentId: string
  readonly reason?: string
}

/** One instruction for a live agent. */
export interface NcagAgentInstructRequest {
  readonly agentId: string
  readonly text: string
}

/** One decision resolution request. */
export interface NcagDecisionResolveRequest {
  readonly decisionId: string
  /** The approach chosen by the operator. */
  readonly outcome: string
  readonly rationale?: string
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/** Result of one submitted work request. */
export interface NcagSubmitValue {
  readonly projectId: string
  readonly tasks: readonly NcagTask[]
}

/** Acknowledgement that an operator control reached the orchestrator. */
export interface NcagControlValue {
  readonly accepted: true
}
