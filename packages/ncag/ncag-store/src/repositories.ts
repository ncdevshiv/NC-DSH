/**
 * Record repositories over the platform database. Each repository maps typed
 * NC-AG records to physical rows. The store owns the connection and audit log;
 * repositories never open transactions spanning unrelated tables.
 * @module @deepseek-ai/dsh-ncag-store/repositories
 */

import type { DatabaseSync } from 'node:sqlite'
import { adoptId } from '@deepseek-ai/dsh-ncag-core'
import type {
  AcceptanceCriterion, AgentInstance, AgentProfile, AgentStatus, Ambiguity, AutonomyPolicy,
  ChangeSet, ChangeSetStatus, DecisionCategory, DecisionRecord, DecisionStatus, DeliveryStatus,
  FindingSeverity, FindingStatus, Instant, IntentAnalysis, LeaseStatus, MessageTarget,
  MessageType, NcagMessage, Project, ProjectContext, ProjectStatus, RequestStatus, Requirement,
  RequirementCategory, RequirementStatus, ResourceLease, ReviewFinding, ReviewKind, ReviewRun, Task,
  TaskStatus, UsageTotals, UserRequest, VerificationEvidence, VerificationMethod,
  VerificationStatus, WorkspaceRecord,
} from '@deepseek-ai/dsh-ncag-core'
import { asRows, decodeBool, decodeJson, encodeBool, encodeJson } from './columns.ts'
import * as sql from './sql.ts'

/** Zero usage used when no accounting exists yet. */
export const EMPTY_USAGE: UsageTotals = { inputTokens: 0, outputTokens: 0, cachedTokens: 0, costUsd: 0, calls: 0 }

/**
 * Sum two usage totals.
 * @param a - accumulated totals.
 * @param b - totals to add.
 * @returns the new totals.
 */
export function addUsage(a: UsageTotals, b: UsageTotals): UsageTotals {
  return {
    inputTokens: a.inputTokens + b.inputTokens,
    outputTokens: a.outputTokens + b.outputTokens,
    cachedTokens: a.cachedTokens + b.cachedTokens,
    costUsd: Number((a.costUsd + b.costUsd).toFixed(6)),
    calls: a.calls + b.calls,
  }
}

/** Default autonomy policy of a project that configures nothing. */
export const DEFAULT_AUTONOMY: AutonomyPolicy = {
  requireArchitecturalApproval: true,
  autonomousSpawning: true,
  challengeOnConflict: true,
  requireVerification: true,
  maxCostUsd: null,
  maxConcurrentAgents: 6,
  maxAgentsPerTask: 3,
  maxRetries: 3,
  maxReviewCycles: 3,
  maxTaskDurationMs: 3600000,
  modelPolicy: 'balanced',
  preferredProviders: [],
}

/** One stored tool execution record. */
export interface ToolCallRecord {
  readonly sequence: number
  readonly at: Instant
  readonly agentId: string
  readonly projectId: string
  readonly taskId: string | null
  readonly tool: string
  readonly args: string
  readonly ok: boolean
  readonly error: string | null
  readonly output: string
  readonly durationMs: number
  readonly correlationId: string
}

/** One checkpoint row restoring execution state after a restart. */
export interface CheckpointRecord {
  readonly id: string
  readonly projectId: string
  readonly at: Instant
  readonly reason: string
  readonly state: Record<string, unknown>
}

/** Physical project row. */
interface ProjectRow {
  readonly id: string
  readonly name: string
  readonly description: string
  readonly repository_path: string
  readonly base_branch: string
  readonly status: string
  readonly instructions: string
  readonly autonomy: string
  readonly context: string
  readonly checks: string
  readonly usage: string
  readonly created_at: string
  readonly updated_at: string
}

/** Map one project row to a {@link Project}. */
function projectFromRow(row: ProjectRow): Project {
  return {
    id: adoptId('project', row.id),
    name: row.name,
    description: row.description,
    repositoryPath: row.repository_path,
    baseBranch: row.base_branch,
    status: row.status as ProjectStatus,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
    autonomy: decodeJson<AutonomyPolicy>(row.autonomy),
    instructions: row.instructions,
    context: decodeJson<ProjectContext>(row.context),
    checks: decodeJson<Project['checks']>(row.checks, {}),
    usage: decodeJson<UsageTotals>(row.usage),
  }
}

/** Physical request row. */
interface RequestRow {
  readonly id: string
  readonly project_id: string
  readonly raw_text: string
  readonly created_at: string
  readonly intent: string
  readonly ambiguities: string
  readonly status: string
}

/** Physical requirement row. */
interface RequirementRow {
  readonly id: string
  readonly request_id: string
  readonly description: string
  readonly category: string
  readonly priority: string
  readonly source: string
  readonly dependencies: string
  readonly acceptance: string
  readonly status: string
}

/** Project records. */
export class ProjectRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one project record.
   * @param project - the record to store.
   */
  save(project: Project): void {
    this.db.prepare(sql.INSERT_PROJECT).run(
      project.id, project.name, project.description, project.repositoryPath, project.baseBranch,
      project.status, project.instructions, encodeJson(project.autonomy), encodeJson(project.context), encodeJson(project.checks),
      encodeJson(project.usage), project.createdAt, project.updatedAt,
    )
  }

  /**
   * Read one project.
   * @param id - project identifier.
   * @returns the project, or null when it does not exist.
   */
  get(id: string): Project | null {
    const row = this.db.prepare(sql.SELECT_PROJECT).get(id) as ProjectRow | undefined
    return row === undefined ? null : projectFromRow(row)
  }

  /**
   * Read every project, newest first.
   * @returns the projects.
   */
  list(): Project[] {
    return (asRows<ProjectRow>(this.db.prepare(sql.SELECT_ALL_PROJECTS).all())).map(projectFromRow)
  }
}

/** Request and requirement records. */
export class RequestRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one request record.
   * @param request - the record to store.
   */
  save(request: UserRequest): void {
    this.db.prepare(sql.INSERT_REQUEST).run(
      request.id, request.projectId, request.rawText, request.createdAt,
      encodeJson(request.intent), encodeJson(request.ambiguities), request.status,
    )
  }

  /**
   * Read one request.
   * @param id - request identifier.
   * @returns the request with its requirements, or null when it does not exist.
   */
  get(id: string): UserRequest | null {
    const row = this.db.prepare(sql.SELECT_REQUEST).get(id) as RequestRow | undefined
    if (row === undefined) return null
    return {
      id: adoptId('request', row.id),
      projectId: adoptId('project', row.project_id),
      rawText: row.raw_text,
      createdAt: row.created_at,
      intent: decodeJson<IntentAnalysis>(row.intent),
      ambiguities: decodeJson<Ambiguity[]>(row.ambiguities, []),
      requirements: RequirementRepository.read(this.db, row.id),
      status: row.status as RequestStatus,
    }
  }

  /**
   * Read a project's requests, newest first.
   * @param projectId - project identifier.
   * @returns the requests.
   */
  listForProject(projectId: string): UserRequest[] {
    const rows = asRows<RequestRow>(this.db.prepare(sql.SELECT_PROJECT_REQUESTS).all(projectId))
    return rows.map(row => this.get(row.id) as UserRequest)
  }
}

/** Requirement records, keyed by request. */
export class RequirementRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace every requirement of one request and delete removed rows.
   * @param projectId - owning project.
   * @param requestId - owning request.
   * @param requirements - the current requirement set.
   */
  saveAll(projectId: string, requestId: string, requirements: readonly Requirement[]): void {
    const keep = new Set(requirements.map(requirement => requirement.id))
    for (const row of this.db.prepare(sql.SELECT_REQUEST_IDS).all(requestId) as Array<{ id: string }>) {
      if (!keep.has(adoptId('requirement', row.id))) this.db.prepare(sql.DELETE_REQUIREMENT).run(row.id)
    }
    const insert = this.db.prepare(sql.INSERT_REQUIREMENT)
    for (const requirement of requirements) {
      insert.run(
        requirement.id, requestId, projectId, requirement.description, requirement.category,
        requirement.priority, requirement.source, encodeJson(requirement.dependencies),
        encodeJson(requirement.acceptanceCriteria), requirement.status,
      )
    }
  }

  /**
   * Read every requirement of one project, in insertion order.
   * @param projectId - owning project.
   * @returns the requirements across the project's requests.
   */
  listForProject(projectId: string): Requirement[] {
    return requirementFromRows(asRows<RequirementRow>(this.db.prepare(sql.SELECT_PROJECT_REQUIREMENTS).all(projectId)))
  }

  /**
   * Read every requirement of one request, in insertion order.
   * @param db - open platform database.
   * @param requestId - owning request.
   * @returns the requirements.
   */
  static read(db: DatabaseSync, requestId: string): Requirement[] {
    return requirementFromRows(asRows<RequirementRow>(db.prepare(sql.SELECT_REQUEST_REQUIREMENTS).all(requestId)))
  }
}

/** Map requirement rows to requirement records. */
function requirementFromRows(rows: readonly RequirementRow[]): Requirement[] {
  return rows.map(row => ({
    id: adoptId('requirement', row.id),
    requestId: adoptId('request', row.request_id),
    description: row.description,
    category: row.category as RequirementCategory,
    priority: row.priority as Requirement['priority'],
    source: row.source,
    dependencies: decodeJson<string[]>(row.dependencies, []).map(id => adoptId('requirement', id)),
    acceptanceCriteria: decodeJson<AcceptanceCriterion[]>(row.acceptance, []),
    status: row.status as RequirementStatus,
  }))
}

/** Physical agent row. */
interface AgentRow {
  readonly id: string
  readonly profile_id: string
  readonly profile_key: string
  readonly project_id: string
  readonly task_id: string | null
  readonly status: string
  readonly status_reason: string | null
  readonly workspace_path: string | null
  readonly branch: string | null
  readonly model: string
  readonly capabilities: string
  readonly usage: string
  readonly failure_count: number
  readonly steps: number
  readonly active_correlation: string | null
  readonly created_at: string
  readonly updated_at: string
  readonly last_heartbeat_at: string
  readonly last_activity_at: string
}

/** Map one agent row to an {@link AgentInstance}. */
function agentFromRow(row: AgentRow): AgentInstance {
  return {
    id: adoptId('agent', row.id),
    profileId: adoptId('profile', row.profile_id),
    profileKey: row.profile_key,
    projectId: adoptId('project', row.project_id),
    taskId: row.task_id === null ? null : adoptId('task', row.task_id),
    status: row.status as AgentStatus,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
    lastHeartbeatAt: row.last_heartbeat_at,
    lastActivityAt: row.last_activity_at,
    workspacePath: row.workspace_path,
    branch: row.branch,
    model: decodeJson<AgentInstance['model']>(row.model),
    capabilities: decodeJson<string[]>(row.capabilities, []),
    usage: decodeJson<UsageTotals>(row.usage, EMPTY_USAGE),
    failureCount: row.failure_count,
    statusReason: row.status_reason,
    activeCorrelationId: row.active_correlation === null ? null : adoptId('correlation', row.active_correlation),
    steps: row.steps,
  }
}

/** Agent instance records. */
export class AgentRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one agent record.
   * @param agent - the record to store.
   */
  save(agent: AgentInstance): void {
    this.db.prepare(sql.INSERT_AGENT).run(
      agent.id, agent.profileId, agent.profileKey, agent.projectId, agent.taskId,
      agent.status, agent.statusReason, agent.workspacePath, agent.branch,
      encodeJson(agent.model), encodeJson(agent.capabilities), encodeJson(agent.usage),
      agent.failureCount, agent.steps, agent.activeCorrelationId,
      agent.createdAt, agent.updatedAt, agent.lastHeartbeatAt, agent.lastActivityAt, null,
    )
  }

  /**
   * Read one agent.
   * @param id - agent identifier.
   * @returns the agent, or null when it does not exist.
   */
  get(id: string): AgentInstance | null {
    const row = this.db.prepare(sql.SELECT_AGENT).get(id) as AgentRow | undefined
    return row === undefined ? null : agentFromRow(row)
  }

  /**
   * Read a project's agents.
   * @param projectId - project identifier.
   * @returns the agents in creation order.
   */
  listForProject(projectId: string): AgentInstance[] {
    return (asRows<AgentRow>(this.db.prepare(sql.SELECT_PROJECT_AGENTS).all(projectId))).map(agentFromRow)
  }

  /**
   * Read every agent that is not stopped or failed, across all projects.
   * @returns the live agents.
   */
  listLive(): AgentInstance[] {
    return (asRows<AgentRow>(this.db.prepare(sql.SELECT_LIVE_AGENTS).all('STOPPED', 'FAILED'))).map(agentFromRow)
  }

  /**
   * Delete an agent record.
   * @param id - agent identifier.
   */
  remove(id: string): void {
    this.db.prepare(sql.DELETE_AGENT).run(id)
  }
}

/** Physical task row. */
interface TaskRow {
  readonly id: string
  readonly project_id: string
  readonly request_id: string | null
  readonly requirement_ids: string
  readonly title: string
  readonly description: string
  readonly required_capability: string
  readonly assigned_agent_id: string | null
  readonly dependency_ids: string
  readonly required_inputs: string
  readonly expected_outputs: string
  readonly acceptance_criteria: string
  readonly required_permissions: string
  readonly context_refs: string
  readonly priority: string
  readonly deadline_ms: number | null
  readonly required_reviews: string
  readonly required_verifications: string
  readonly status: string
  readonly status_reason: string | null
  readonly depth: number
  readonly attempts: number
  readonly review_cycles: number
  readonly consultation: number
  readonly created_at: string
  readonly updated_at: string
  readonly started_at: string | null
  readonly completed_at: string | null
}

/** Map one task row to a {@link Task}. */
function taskFromRow(row: TaskRow): Task {
  return {
    id: adoptId('task', row.id),
    projectId: adoptId('project', row.project_id),
    requestId: row.request_id === null ? null : adoptId('request', row.request_id),
    requirementIds: decodeJson<string[]>(row.requirement_ids, []).map(id => adoptId('requirement', id)),
    title: row.title,
    description: row.description,
    requiredCapability: row.required_capability,
    assignedAgentId: row.assigned_agent_id === null ? null : adoptId('agent', row.assigned_agent_id),
    dependencyIds: decodeJson<string[]>(row.dependency_ids, []).map(id => adoptId('task', id)),
    requiredInputs: decodeJson<string[]>(row.required_inputs, []),
    expectedOutputs: decodeJson<string[]>(row.expected_outputs, []),
    acceptanceCriteria: decodeJson<AcceptanceCriterion[]>(row.acceptance_criteria, []),
    requiredPermissions: decodeJson<Task['requiredPermissions']>(row.required_permissions, []),
    contextRefs: decodeJson<string[]>(row.context_refs, []),
    priority: row.priority as Task['priority'],
    deadlineMs: row.deadline_ms,
    requiredReviews: decodeJson<ReviewKind[]>(row.required_reviews, []),
    requiredVerifications: decodeJson<VerificationMethod[]>(row.required_verifications, []),
    status: row.status as TaskStatus,
    depth: row.depth,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
    startedAt: row.started_at,
    completedAt: row.completed_at,
    attempts: row.attempts,
    reviewCycles: row.review_cycles,
    consultation: decodeBool(row.consultation),
    statusReason: row.status_reason,
  }
}

/** Task records. */
export class TaskRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one task record.
   * @param task - the record to store.
   */
  save(task: Task): void {
    this.db.prepare(sql.INSERT_TASK).run(
      task.id, task.projectId, task.requestId, encodeJson(task.requirementIds), task.title,
      task.description, task.requiredCapability, task.assignedAgentId, encodeJson(task.dependencyIds),
      encodeJson(task.requiredInputs), encodeJson(task.expectedOutputs), encodeJson(task.acceptanceCriteria),
      encodeJson(task.requiredPermissions), encodeJson(task.contextRefs), task.priority, task.deadlineMs,
      encodeJson(task.requiredReviews), encodeJson(task.requiredVerifications), task.status,
      task.statusReason, task.depth, task.attempts, task.reviewCycles, encodeBool(task.consultation),
      task.createdAt, task.updatedAt, task.startedAt, task.completedAt,
    )
  }

  /**
   * Read one task.
   * @param id - task identifier.
   * @returns the task, or null when it does not exist.
   */
  get(id: string): Task | null {
    const row = this.db.prepare(sql.SELECT_TASK).get(id) as TaskRow | undefined
    return row === undefined ? null : taskFromRow(row)
  }

  /**
   * Read a project's tasks.
   * @param projectId - project identifier.
   * @returns the tasks ordered by depth then creation.
   */
  listForProject(projectId: string): Task[] {
    return (asRows<TaskRow>(this.db.prepare(sql.SELECT_PROJECT_TASKS).all(projectId))).map(taskFromRow)
  }

  /**
   * Delete a task record.
   * @param id - task identifier.
   */
  remove(id: string): void {
    this.db.prepare(sql.DELETE_TASK).run(id)
  }
}

/** Physical profile row. */
interface ProfileRow {
  readonly id: string
  readonly key: string
  readonly payload: string
  readonly built_in: number
}

/** Agent profile registry records. */
export class ProfileRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one profile.
   * @param profile - the profile to store.
   */
  save(profile: AgentProfile): void {
    const { id, key, builtIn, ...rest } = profile
    this.db.prepare(sql.INSERT_PROFILE).run(id, key, encodeJson({ ...rest, key }), encodeBool(builtIn))
  }

  /**
   * Read one profile by identifier.
   * @param id - profile identifier.
   * @returns the profile, or null when it does not exist.
   */
  get(id: string): AgentProfile | null {
    const row = this.db.prepare(sql.SELECT_PROFILE).get(id) as ProfileRow | undefined
    return row === undefined ? null : profileFromRow(row)
  }

  /**
   * Read one profile by registry key.
   * @param key - registry key.
   * @returns the profile, or null when it does not exist.
   */
  getByKey(key: string): AgentProfile | null {
    const row = this.db.prepare(sql.SELECT_PROFILE_BY_KEY).get(key) as ProfileRow | undefined
    return row === undefined ? null : profileFromRow(row)
  }

  /**
   * Read every registered profile.
   * @returns built-in profiles first, then created profiles by key.
   */
  list(): AgentProfile[] {
    return (asRows<ProfileRow>(this.db.prepare(sql.SELECT_ALL_PROFILES).all())).map(profileFromRow)
  }

  /**
   * Delete one profile.
   * @param id - profile identifier.
   */
  remove(id: string): void {
    this.db.prepare(sql.DELETE_PROFILE).run(id)
  }
}

/** Map one profile row to an {@link AgentProfile}. */
function profileFromRow(row: ProfileRow): AgentProfile {
  const payload = decodeJson<Omit<AgentProfile, 'id' | 'builtIn'>>(row.payload)
  return { ...payload, id: adoptId('profile', row.id), builtIn: decodeBool(row.built_in) }
}

/** Physical message row. */
interface MessageRow {
  readonly id: string
  readonly at: string
  readonly sender: string | null
  readonly target: string
  readonly project_id: string
  readonly task_id: string | null
  readonly type: string
  readonly subject: string
  readonly content: string
  readonly context_refs: string
  readonly correlation_id: string
  readonly priority: string
  readonly requires_ack: number
  readonly delivery_status: string
  readonly delivered_at: string | null
  readonly acknowledged_at: string | null
  readonly attempts: number
  readonly expires_at_ms: number | null
  readonly dead_letter_reason: string | null
}

/** Map one message row to a {@link NcagMessage}. */
function messageFromRow(row: MessageRow): NcagMessage {
  return {
    id: adoptId('message', row.id),
    timestamp: row.at,
    sender: row.sender === null ? null : adoptId('agent', row.sender),
    target: decodeJson<MessageTarget>(row.target),
    projectId: adoptId('project', row.project_id),
    taskId: row.task_id === null ? null : adoptId('task', row.task_id),
    type: row.type as MessageType,
    subject: row.subject,
    content: row.content,
    contextRefs: decodeJson<string[]>(row.context_refs, []),
    correlationId: adoptId('correlation', row.correlation_id),
    priority: row.priority as NcagMessage['priority'],
    requiresAck: decodeBool(row.requires_ack),
    deliveryStatus: row.delivery_status as DeliveryStatus,
    deliveredAt: row.delivered_at,
    acknowledgedAt: row.acknowledged_at,
    attempts: row.attempts,
    expiresAtMs: row.expires_at_ms === null ? null : row.expires_at_ms,
  }
}

/** Durable message records. */
export class MessageRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Store one message.
   * @param message - the message to store.
   */
  insert(message: NcagMessage): void {
    this.db.prepare(sql.INSERT_MESSAGE).run(
      message.id, message.timestamp, message.sender, encodeJson(message.target), message.projectId,
      message.taskId, message.type, message.subject, message.content, encodeJson(message.contextRefs),
      message.correlationId, message.priority, encodeBool(message.requiresAck), message.deliveryStatus,
      message.deliveredAt, message.acknowledgedAt, message.attempts, message.expiresAtMs, null,
    )
  }

  /**
   * Read one message.
   * @param id - message identifier.
   * @returns the message, or null when it does not exist.
   */
  get(id: string): NcagMessage | null {
    const row = this.db.prepare(sql.SELECT_MESSAGE).get(id) as MessageRow | undefined
    return row === undefined ? null : messageFromRow(row)
  }

  /**
   * Read a project's messages in insertion order.
   * @param projectId - project identifier.
   * @returns the messages.
   */
  listForProject(projectId: string): NcagMessage[] {
    return (asRows<MessageRow>(this.db.prepare(sql.SELECT_PROJECT_MESSAGES).all(projectId))).map(messageFromRow)
  }

  /**
   * Read a project's messages recorded for one correlation identifier.
   * @param projectId - project identifier.
   * @param correlationId - correlation identifier tying the messages to one operation.
   * @returns the matching messages in insertion order.
   */
  forCorrelation(projectId: string, correlationId: string): NcagMessage[] {
    return (asRows<MessageRow>(this.db.prepare(
      'SELECT * FROM messages WHERE project_id = ? AND correlation_id = ? ORDER BY at ASC, id ASC',
    ).all(projectId, correlationId))).map(messageFromRow)
  }

  /**
   * Read pending messages awaiting delivery.
   * @param limit - maximum number to read.
   * @returns the pending messages, oldest first.
   */
  pending(limit: number): NcagMessage[] {
    return (asRows<MessageRow>(this.db.prepare(sql.SELECT_UNDELIVERED).all('PENDING', limit))).map(messageFromRow)
  }

  /**
   * Persist one message delivery update.
   * @param message - the message carrying the new delivery state.
   * @param deadLetterReason - reason recorded when the message is dead-lettered.
   */
  updateDelivery(message: NcagMessage, deadLetterReason: string | null): void {
    this.db.prepare(sql.UPDATE_MESSAGE_DELIVERY).run(
      message.deliveryStatus, message.deliveredAt, message.acknowledgedAt, message.attempts,
      deadLetterReason, message.id,
    )
  }

  /**
   * Record an acknowledgement of one message by one agent.
   * @param messageId - message identifier.
   * @param agentId - acknowledging agent.
   * @returns whether this call created the acknowledgement.
   */
  acknowledge(messageId: string, agentId: string): boolean {
    const before = this.db.prepare(sql.SELECT_MESSAGE_ACK).get(messageId, agentId)
    if (before !== undefined) return false
    this.db.prepare(sql.ACK_MESSAGE).run(messageId, agentId, new Date().toISOString())
    return true
  }

  /**
   * Read an agent's delivery cursor.
   * @param agentId - agent identifier.
   * @returns the last sequence seen, or 0 when the agent has no cursor.
   */
  cursor(agentId: string): number {
    const row = this.db.prepare(sql.SELECT_CURSOR).get(agentId) as { last_seen: number } | undefined
    return row === undefined ? 0 : row.last_seen
  }

  /**
   * Advance an agent's delivery cursor.
   * @param agentId - agent identifier.
   * @param lastSeen - highest conversation position the agent has consumed.
   */
  setCursor(agentId: string, lastSeen: number): void {
    this.db.prepare(sql.UPSERT_CURSOR).run(agentId, lastSeen, new Date().toISOString())
  }
}

/** Physical decision row. */
interface DecisionRow {
  readonly id: string
  readonly project_id: string
  readonly task_id: string | null
  readonly category: string
  readonly author: string
  readonly problem: string
  readonly debate: string | null
  readonly recommendation: string
  readonly outcome: string
  readonly rationale: string | null
  readonly rejected_recommendation: string | null
  readonly consequences: string
  readonly affected_modules: string
  readonly affected_task_ids: string
  readonly status: string
  readonly review_history: string
  readonly created_at: string
  readonly decided_at: string | null
}

/** Map one decision row to a {@link DecisionRecord}. */
function decisionFromRow(row: DecisionRow): DecisionRecord {
  return {
    id: adoptId('decision', row.id),
    projectId: adoptId('project', row.project_id),
    taskId: row.task_id === null ? null : adoptId('task', row.task_id),
    category: row.category as DecisionCategory,
    author: row.author,
    problem: row.problem,
    debate: decodeJson<DecisionRecord['debate']>(row.debate, null),
    recommendation: row.recommendation,
    outcome: row.outcome,
    rationale: row.rationale,
    rejectedRecommendation: row.rejected_recommendation,
    consequences: decodeJson<string[]>(row.consequences, []),
    affectedModules: decodeJson<string[]>(row.affected_modules, []),
    affectedTaskIds: decodeJson<string[]>(row.affected_task_ids, []).map(id => adoptId('task', id)),
    status: row.status as DecisionStatus,
    createdAt: row.created_at,
    decidedAt: row.decided_at,
    reviewHistory: decodeJson<string[]>(row.review_history, []),
  }
}

/** Decision records. */
export class DecisionRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one decision record.
   * @param decision - the record to store.
   */
  save(decision: DecisionRecord): void {
    this.db.prepare(sql.INSERT_DECISION).run(
      decision.id, decision.projectId, decision.taskId, decision.category, decision.author,
      decision.problem, decision.debate === null ? null : encodeJson(decision.debate),
      decision.recommendation, decision.outcome, decision.rationale, decision.rejectedRecommendation,
      encodeJson(decision.consequences), encodeJson(decision.affectedModules),
      encodeJson(decision.affectedTaskIds), decision.status, encodeJson(decision.reviewHistory),
      decision.createdAt, decision.decidedAt,
    )
  }

  /**
   * Read one decision.
   * @param id - decision identifier.
   * @returns the decision, or null when it does not exist.
   */
  get(id: string): DecisionRecord | null {
    const row = this.db.prepare(sql.SELECT_DECISION).get(id) as DecisionRow | undefined
    return row === undefined ? null : decisionFromRow(row)
  }

  /**
   * Read a project's decisions in creation order.
   * @param projectId - project identifier.
   * @returns the decisions.
   */
  listForProject(projectId: string): DecisionRecord[] {
    return (asRows<DecisionRow>(this.db.prepare(sql.SELECT_PROJECT_DECISIONS).all(projectId))).map(decisionFromRow)
  }

  /**
   * Read decisions waiting for a user answer.
   * @returns the pending decisions.
   */
  pending(): DecisionRecord[] {
    return (asRows<DecisionRow>(this.db.prepare(sql.SELECT_PENDING_DECISIONS).all('AWAITING_USER'))).map(decisionFromRow)
  }
}

/** Physical finding row. */
interface FindingRow {
  readonly id: string
  readonly project_id: string
  readonly task_id: string
  readonly reviewer_agent_id: string
  readonly review_kind: string
  readonly severity: string
  readonly description: string
  readonly evidence: string
  readonly artifact: string
  readonly impact: string
  readonly recommendation: string
  readonly verification: string
  readonly status: string
  readonly created_at: string
  readonly resolved_at: string | null
}

/** Map one finding row to a {@link ReviewFinding}. */
function findingFromRow(row: FindingRow): ReviewFinding {
  return {
    id: adoptId('finding', row.id),
    taskId: adoptId('task', row.task_id),
    projectId: adoptId('project', row.project_id),
    reviewerAgentId: adoptId('agent', row.reviewer_agent_id),
    reviewKind: row.review_kind as ReviewFinding['reviewKind'],
    severity: row.severity as FindingSeverity,
    description: row.description,
    evidence: row.evidence,
    artifact: row.artifact,
    impact: row.impact,
    recommendation: row.recommendation,
    verificationMethod: row.verification,
    status: row.status as FindingStatus,
    createdAt: row.created_at,
    resolvedAt: row.resolved_at,
  }
}

/** Review finding records. */
export class FindingRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one finding.
   * @param finding - the finding to store.
   */
  save(finding: ReviewFinding): void {
    this.db.prepare(sql.INSERT_FINDING).run(
      finding.id, finding.projectId, finding.taskId, finding.reviewerAgentId, finding.reviewKind, finding.severity,
      finding.description, finding.evidence, finding.artifact, finding.impact, finding.recommendation,
      finding.verificationMethod, finding.status, finding.createdAt, finding.resolvedAt,
    )
  }

  /**
   * Read a task's findings.
   * @param taskId - task identifier.
   * @returns the findings in creation order.
   */
  listForTask(taskId: string): ReviewFinding[] {
    return (asRows<FindingRow>(this.db.prepare(sql.SELECT_TASK_FINDINGS).all(taskId))).map(findingFromRow)
  }

  /**
   * Read every open finding.
   * @returns the open findings.
   */
  open(): ReviewFinding[] {
    return (asRows<FindingRow>(this.db.prepare(sql.SELECT_OPEN_FINDINGS).all('OPEN'))).map(findingFromRow)
  }
}

/** Physical review-run row. */
interface ReviewRunRow {
  readonly id: string
  readonly project_id: string
  readonly task_id: string
  readonly review_kind: string
  readonly reviewer_agent_id: string
  readonly summary: string
  readonly findings: number
  readonly rejected: number
  readonly created_at: string
}

/** Map one review-run row to a {@link ReviewRun}. */
function reviewRunFromRow(row: ReviewRunRow): ReviewRun {
  return {
    id: row.id,
    projectId: adoptId('project', row.project_id),
    taskId: adoptId('task', row.task_id),
    reviewKind: row.review_kind as ReviewKind,
    reviewerAgentId: adoptId('agent', row.reviewer_agent_id),
    summary: row.summary,
    findings: row.findings,
    rejected: decodeBool(row.rejected),
    createdAt: row.created_at,
  }
}

/** Executed review runs, one row per review kind the orchestrator ran. */
export class ReviewRunRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Record one executed review.
   * @param run - the review run to store.
   */
  save(run: ReviewRun): void {
    this.db.prepare(sql.INSERT_REVIEW_RUN).run(
      run.id, run.projectId, run.taskId, run.reviewKind, run.reviewerAgentId,
      run.summary, run.findings, encodeBool(run.rejected), run.createdAt,
    )
  }

  /**
   * Read the reviews executed for one task.
   * @param taskId - task identifier.
   * @returns the runs in execution order.
   */
  listForTask(taskId: string): ReviewRun[] {
    return (asRows<ReviewRunRow>(this.db.prepare(sql.SELECT_TASK_REVIEW_RUNS).all(taskId))).map(reviewRunFromRow)
  }

  /**
   * Read the reviews executed in one project.
   * @param projectId - project identifier.
   * @returns the runs in execution order.
   */
  listForProject(projectId: string): ReviewRun[] {
    return (asRows<ReviewRunRow>(this.db.prepare(sql.SELECT_PROJECT_REVIEW_RUNS).all(projectId))).map(reviewRunFromRow)
  }
}

/** Physical evidence row. */
interface EvidenceRow {
  readonly sequence: number
  readonly task_id: string
  readonly criterion_id: string
  readonly method: string
  readonly status: string
  readonly command: string | null
  readonly exit_code: number | null
  readonly output: string
  readonly recorded_at: string
  readonly produced_by: string | null
}

/** Verification evidence records. */
export class EvidenceRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Append one evidence record.
   * @param projectId - owning project.
   * @param evidence - the evidence to store.
   */
  append(projectId: string, evidence: VerificationEvidence): void {
    this.db.prepare(sql.INSERT_EVIDENCE).run(
      evidence.taskId, projectId, evidence.criterionId, evidence.method, evidence.status,
      evidence.command, evidence.exitCode, evidence.output, evidence.recordedAt, evidence.producedBy,
    )
  }

  /**
   * Read a task's evidence.
   * @param taskId - task identifier.
   * @returns the evidence in recording order.
   */
  listForTask(taskId: string): Array<VerificationEvidence & { sequence: number }> {
    return asRows<EvidenceRow>(this.db.prepare(sql.SELECT_TASK_EVIDENCE).all(taskId)).map(row => ({
      taskId: adoptId('task', row.task_id),
      criterionId: row.criterion_id,
      method: row.method as VerificationMethod,
      status: row.status as VerificationStatus,
      command: row.command,
      exitCode: row.exit_code,
      output: row.output,
      recordedAt: row.recorded_at,
      producedBy: row.produced_by === null ? null : adoptId('agent', row.produced_by),
      sequence: row.sequence,
    }))
  }
}

/** Physical lease row. */
interface LeaseRow {
  readonly id: string
  readonly project_id: string
  readonly resource_path: string
  readonly mode: string
  readonly agent_id: string
  readonly task_id: string | null
  readonly fencing_token: number
  readonly acquired_at_ms: number
  readonly renewed_at_ms: number
  readonly expires_at_ms: number
  readonly status: string
  readonly reason: string
}

/** Map one lease row to a {@link ResourceLease}. */
function leaseFromRow(row: LeaseRow): ResourceLease {
  return {
    id: adoptId('lease', row.id),
    projectId: adoptId('project', row.project_id),
    resourcePath: row.resource_path,
    mode: row.mode as ResourceLease['mode'],
    agentId: adoptId('agent', row.agent_id),
    taskId: row.task_id === null ? null : adoptId('task', row.task_id),
    fencingToken: row.fencing_token,
    acquiredAtMs: row.acquired_at_ms,
    renewedAtMs: row.renewed_at_ms,
    expiresAtMs: row.expires_at_ms,
    status: row.status as LeaseStatus,
    reason: row.reason,
  }
}

/** Resource lease records. */
export class LeaseRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Store one lease.
   * @param lease - the lease to store.
   */
  insert(lease: ResourceLease): void {
    this.db.prepare(sql.INSERT_LEASE).run(
      lease.id, lease.projectId, lease.resourcePath, lease.mode, lease.agentId, lease.taskId,
      lease.fencingToken, lease.acquiredAtMs, lease.renewedAtMs, lease.expiresAtMs, lease.status,
      lease.reason,
    )
  }

  /**
   * Read one lease.
   * @param id - lease identifier.
   * @returns the lease, or null when it does not exist.
   */
  get(id: string): ResourceLease | null {
    const row = this.db.prepare(sql.SELECT_LEASE).get(id) as LeaseRow | undefined
    return row === undefined ? null : leaseFromRow(row)
  }

  /**
   * Read a project's active leases.
   * @param projectId - project identifier.
   * @returns the active leases ordered by path.
   */
  active(projectId: string): ResourceLease[] {
    return (asRows<LeaseRow>(this.db.prepare(sql.SELECT_ACTIVE_LEASES).all(projectId, 'ACTIVE'))).map(leaseFromRow)
  }

  /**
   * Persist a lease state change.
   * @param lease - the lease carrying the new state.
   */
  update(lease: ResourceLease): void {
    this.db.prepare(sql.UPDATE_LEASE).run(
      lease.status, lease.renewedAtMs, lease.expiresAtMs, lease.fencingToken, lease.id,
    )
  }

  /**
   * Read the highest fencing token issued in a project.
   * @param projectId - project identifier.
   * @returns the highest token, or 0 when the project has no leases.
   */
  maxFencingToken(projectId: string): number {
    const row = this.db.prepare(sql.SELECT_MAX_FENCING).get(projectId) as { token: number }
    return row.token
  }

  /**
   * Release every active lease of one agent.
   * @param agentId - agent identifier.
   * @param status - terminal status to record.
   */
  releaseForAgent(agentId: string, status: LeaseStatus): void {
    this.db.prepare(sql.DELETE_LEASES_FOR_AGENT).run(status, agentId, 'ACTIVE')
  }

  /**
   * Record one agent waiting for a resource another agent holds.
   *
   * The waiter is durable, so an operator can see who is queued behind an
   * occupied path, and a repeated request keeps its original queue position.
   * @param request - who waits, for which path and in which mode.
   */
  wait(request: {
    readonly projectId: string
    readonly resourcePath: string
    readonly agentId: string
    readonly taskId: string | null
    readonly mode: ResourceLease['mode']
    readonly reason: string
  }): void {
    this.db.prepare(sql.RECORD_LEASE_REQUEST).run(
      request.projectId, request.resourcePath, request.agentId, request.taskId,
      request.mode, request.reason, new Date().toISOString(),
    )
  }

  /**
   * Read the agents waiting for one path in one mode.
   * @param projectId - project identifier.
   * @param resourcePath - the held path.
   * @param mode - lease mode the waiters asked for.
   * @returns the waiters in request order.
   */
  waiters(projectId: string, resourcePath: string, mode: ResourceLease['mode']): LeaseWaiter[] {
    return asRows<LeaseWaiterRow>(this.db.prepare(sql.SELECT_LEASE_WAITERS).all(projectId, resourcePath, mode))
      .map(row => ({
        projectId: adoptId('project', row.project_id),
        resourcePath: row.resource_path,
        agentId: adoptId('agent', row.agent_id),
        taskId: row.task_id === null ? null : adoptId('task', row.task_id),
        mode: row.mode as ResourceLease['mode'],
        reason: row.reason,
        requestedAt: row.requested_at,
      }))
  }

  /**
   * Read every agent waiting anywhere in a project.
   *
   * A writer waiting for a path that readers hold asks in a different mode
   * from the holders, so the whole project queue is read rather than only the
   * modes currently granted.
   * @param projectId - project identifier.
   * @returns the waiters in request order.
   */
  waitingForProject(projectId: string): LeaseWaiter[] {
    return asRows<LeaseWaiterRow>(this.db.prepare(sql.SELECT_PROJECT_LEASE_WAITERS).all(projectId))
      .map(row => ({
        projectId: adoptId('project', row.project_id),
        resourcePath: row.resource_path,
        agentId: adoptId('agent', row.agent_id),
        taskId: row.task_id === null ? null : adoptId('task', row.task_id),
        mode: row.mode as ResourceLease['mode'],
        reason: row.reason,
        requestedAt: row.requested_at,
      }))
  }

  /**
   * Remove one waiter once it holds the resource or gives up.
   * @param projectId - project identifier.
   * @param resourcePath - the path it waited for.
   * @param agentId - the waiting agent.
   * @param mode - lease mode it waited for.
   */
  clearWait(projectId: string, resourcePath: string, agentId: string, mode: ResourceLease['mode']): void {
    this.db.prepare(sql.CLEAR_LEASE_REQUEST).run(projectId, resourcePath, agentId, mode)
  }
}

/** Physical lease-request row. */
interface LeaseWaiterRow {
  readonly project_id: string
  readonly resource_path: string
  readonly agent_id: string
  readonly task_id: string | null
  readonly mode: string
  readonly reason: string
  readonly requested_at: string
}

/** One agent queued behind a held resource. */
export interface LeaseWaiter {
  readonly projectId: string
  readonly resourcePath: string
  readonly agentId: string
  readonly taskId: string | null
  readonly mode: ResourceLease['mode']
  readonly reason: string
  readonly requestedAt: string
}

/** Physical change-set row. */
interface ChangeSetRow {
  readonly id: string
  readonly project_id: string
  readonly task_id: string
  readonly agent_id: string
  readonly workspace_path: string
  readonly branch: string
  readonly base_commit: string
  readonly head_commit: string | null
  readonly changed_files: string
  readonly message: string
  readonly status: string
  readonly submitted_at: string
  readonly report: string
  readonly integrated_commit: string | null
}

/** Map one change-set row to a {@link ChangeSet}. */
function changeSetFromRow(row: ChangeSetRow): ChangeSet {
  return {
    id: adoptId('changeset', row.id),
    projectId: adoptId('project', row.project_id),
    taskId: adoptId('task', row.task_id),
    agentId: adoptId('agent', row.agent_id),
    workspacePath: row.workspace_path,
    branch: row.branch,
    baseCommit: row.base_commit,
    headCommit: row.head_commit,
    changedFiles: decodeJson<string[]>(row.changed_files, []),
    message: row.message,
    status: row.status as ChangeSetStatus,
    submittedAt: row.submitted_at,
    report: decodeJson<string[]>(row.report, []),
    integratedCommit: row.integrated_commit,
  }
}

/** Submitted change sets awaiting or recording integration. */
export class ChangeSetRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one change set.
   * @param changeSet - the record to store.
   */
  save(changeSet: ChangeSet): void {
    this.db.prepare(sql.INSERT_CHANGESET).run(
      changeSet.id, changeSet.projectId, changeSet.taskId, changeSet.agentId, changeSet.workspacePath,
      changeSet.branch, changeSet.baseCommit, changeSet.headCommit, encodeJson(changeSet.changedFiles),
      changeSet.message, changeSet.status, changeSet.submittedAt, encodeJson(changeSet.report),
      changeSet.integratedCommit,
    )
  }

  /**
   * Read one change set.
   * @param id - change set identifier.
   * @returns the change set, or null when it does not exist.
   */
  get(id: string): ChangeSet | null {
    const row = this.db.prepare(sql.SELECT_CHANGESET).get(id) as ChangeSetRow | undefined
    return row === undefined ? null : changeSetFromRow(row)
  }

  /**
   * Read a project's change sets in submission order.
   * @param projectId - project identifier.
   * @returns the change sets.
   */
  listForProject(projectId: string): ChangeSet[] {
    return (asRows<ChangeSetRow>(this.db.prepare(sql.SELECT_PROJECT_CHANGESETS).all(projectId))).map(changeSetFromRow)
  }

  /**
   * Read a project's change sets still awaiting integration.
   * @param projectId - project identifier.
   * @returns the open change sets.
   */
  open(projectId: string): ChangeSet[] {
    return (asRows<ChangeSetRow>(this.db.prepare(sql.SELECT_OPEN_CHANGESETS).all(projectId, 'SUBMITTED', 'VALIDATING'))).map(changeSetFromRow)
  }
}

/** Physical workspace row. */
interface WorkspaceRow {
  readonly agent_id: string
  readonly project_id: string
  readonly task_id: string | null
  readonly path: string
  readonly branch: string
  readonly base_commit: string
  readonly created_at: string
  readonly released_at: string | null
}

/** Agent workspace records. */
export class WorkspaceRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Insert or replace one workspace record.
   * @param workspace - the record to store.
   */
  save(workspace: WorkspaceRecord): void {
    this.db.prepare(sql.INSERT_WORKSPACE).run(
      workspace.agentId, workspace.projectId, workspace.taskId, workspace.path, workspace.branch,
      workspace.baseCommit, workspace.createdAt, workspace.releasedAt,
    )
  }

  /**
   * Read one agent's workspace.
   * @param agentId - agent identifier.
   * @returns the workspace, or null when the agent owns none.
   */
  get(agentId: string): WorkspaceRecord | null {
    const row = this.db.prepare(sql.SELECT_WORKSPACE).get(agentId) as WorkspaceRow | undefined
    if (row === undefined) return null
    return {
      agentId: adoptId('agent', row.agent_id),
      projectId: adoptId('project', row.project_id),
      taskId: row.task_id === null ? null : adoptId('task', row.task_id),
      path: row.path,
      branch: row.branch,
      baseCommit: row.base_commit,
      createdAt: row.created_at,
      releasedAt: row.released_at,
    }
  }

  /**
   * Mark one agent's workspace released.
   * @param agentId - agent identifier.
   */
  release(agentId: string): void {
    this.db.prepare(sql.RELEASE_WORKSPACE).run(new Date().toISOString(), agentId)
  }
}

/** Physical tool-call row. */
interface ToolCallRow {
  readonly sequence: number
  readonly at: string
  readonly agent_id: string
  readonly project_id: string
  readonly task_id: string | null
  readonly tool: string
  readonly args: string
  readonly ok: number
  readonly error: string | null
  readonly output: string
  readonly duration_ms: number
  readonly correlation_id: string
}

/** Tool execution records, one row per call. */
export class ToolCallRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Append one tool execution record.
   * @param record - the execution to store.
   */
  append(record: Omit<ToolCallRecord, 'sequence'>): void {
    this.db.prepare(sql.INSERT_TOOL_CALL).run(
      record.at, record.agentId, record.projectId, record.taskId, record.tool, record.args,
      encodeBool(record.ok), record.error, record.output, record.durationMs, record.correlationId,
    )
  }

  /**
   * Read one agent's most recent tool executions.
   * @param agentId - agent identifier.
   * @param limit - maximum rows.
   * @returns the rows, newest first.
   */
  recent(agentId: string, limit: number): ToolCallRecord[] {
    return (asRows<ToolCallRow>(this.db.prepare(sql.SELECT_TOOL_CALLS).all(agentId, limit))).map(toolCallFromRow)
  }

  /**
   * Read one project's tool executions recorded for a correlation identifier.
   * @param projectId - project identifier.
   * @param correlationId - correlation identifier tying the calls to one operation.
   * @returns the matching executions in sequence order.
   */
  forCorrelation(projectId: string, correlationId: string): ToolCallRecord[] {
    return (asRows<ToolCallRow>(this.db.prepare(
      'SELECT * FROM tool_calls WHERE project_id = ? AND correlation_id = ? ORDER BY sequence ASC',
    ).all(projectId, correlationId))).map(toolCallFromRow)
  }
}

/** Map one tool-call row to a {@link ToolCallRecord}. */
function toolCallFromRow(row: ToolCallRow): ToolCallRecord {
  return {
    sequence: row.sequence,
    at: row.at,
    agentId: row.agent_id,
    projectId: row.project_id,
    taskId: row.task_id,
    tool: row.tool,
    args: row.args,
    ok: decodeBool(row.ok),
    error: row.error,
    output: row.output,
    durationMs: row.duration_ms,
    correlationId: row.correlation_id,
  }
}

/** One usage ledger row, appended per model call. */
export interface UsageRecord {
  readonly at: string
  readonly projectId: string
  readonly taskId: string | null
  readonly agentId: string | null
  readonly provider: string
  readonly model: string
  readonly inputTokens: number
  readonly outputTokens: number
  readonly cachedTokens: number
  readonly costUsd: number
  readonly purpose: string
}

/** Token and cost accounting over the platform database. */
export class UsageRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Append one usage row.
   * @param record - the usage to record.
   */
  append(record: UsageRecord): void {
    this.db.prepare(sql.INSERT_USAGE).run(
      record.at, record.projectId, record.taskId, record.agentId, record.provider, record.model,
      record.inputTokens, record.outputTokens, record.cachedTokens, record.costUsd, record.purpose,
    )
  }

  /**
   * Sum a project's usage.
   * @param projectId - project identifier.
   * @returns the accumulated totals.
   */
  totals(projectId: string): UsageTotals {
    // The aggregate always returns exactly one row, with zero sums when the
    // project has no ledger entries.
    const row = this.db.prepare(sql.SELECT_PROJECT_USAGE).get(projectId) as
      { input_tokens: number; output_tokens: number; cached_tokens: number; cost_usd: number; calls: number }
    return {
      inputTokens: row.input_tokens,
      outputTokens: row.output_tokens,
      cachedTokens: row.cached_tokens,
      costUsd: row.cost_usd,
      calls: row.calls,
    }
  }
}

/** Recovery checkpoints capturing execution state at safe boundaries. */
export class CheckpointRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Store one checkpoint.
   * @param checkpoint - the checkpoint to store.
   */
  save(checkpoint: CheckpointRecord): void {
    this.db.prepare(sql.INSERT_CHECKPOINT).run(
      checkpoint.id, checkpoint.projectId, checkpoint.at, checkpoint.reason, encodeJson(checkpoint.state),
    )
  }

  /**
   * Read a project's checkpoints in recording order.
   * @param projectId - project identifier.
   * @returns the checkpoints.
   */
  listForProject(projectId: string): CheckpointRecord[] {
    const rows = this.db.prepare(sql.SELECT_CHECKPOINTS).all(projectId) as Array<{
      id: string
      project_id: string
      at: string
      reason: string
      state: string
    }>
    return rows.map(row => ({
      id: row.id,
      projectId: row.project_id,
      at: row.at,
      reason: row.reason,
      state: decodeJson<Record<string, unknown>>(row.state, {}),
    }))
  }
}

/** Repeated tool invocation counters used by stuck-agent detection. */
export class ToolInvocationRepository {
  /**
   * @param db - open platform database.
   */
  private readonly db: DatabaseSync

  /**
   * @param db - open platform database.
   */
  constructor(db: DatabaseSync) {
    this.db = db
  }

  /**
   * Record one invocation and return its new count.
   * @param agentId - agent identifier.
   * @param fingerprint - stable hash of the tool name and arguments.
   * @returns the number of identical invocations by this agent.
   */
  record(agentId: string, fingerprint: string): number {
    const at = new Date().toISOString()
    this.db.prepare(sql.UPSERT_TOOL_INVOCATION).run(agentId, fingerprint, at, at)
    const row = this.db.prepare(sql.SELECT_TOOL_INVOCATION).get(agentId, fingerprint) as { count: number }
    return row.count
  }

  /**
   * Clear an agent's counters after progress.
   * @param agentId - agent identifier.
   */
  reset(agentId: string): void {
    this.db.prepare(sql.RESET_TOOL_INVOCATIONS).run(agentId)
  }
}

/** One repository bundle over a single database connection. */
export interface Repositories {
  readonly projects: ProjectRepository
  readonly requests: RequestRepository
  readonly requirements: RequirementRepository
  readonly profiles: ProfileRepository
  readonly agents: AgentRepository
  readonly tasks: TaskRepository
  readonly messages: MessageRepository
  readonly decisions: DecisionRepository
  readonly findings: FindingRepository
  readonly reviews: ReviewRunRepository
  readonly evidence: EvidenceRepository
  readonly leases: LeaseRepository
  readonly changeSets: ChangeSetRepository
  readonly workspaces: WorkspaceRepository
  readonly toolCalls: ToolCallRepository
  readonly usage: UsageRepository
  readonly checkpoints: CheckpointRepository
  readonly toolInvocations: ToolInvocationRepository
}

/**
 * Build the repository bundle over one database connection.
 * @param db - open platform database.
 * @returns every repository sharing the connection.
 */
export function createRepositories(db: DatabaseSync): Repositories {
  return {
    projects: new ProjectRepository(db),
    requests: new RequestRepository(db),
    requirements: new RequirementRepository(db),
    profiles: new ProfileRepository(db),
    agents: new AgentRepository(db),
    tasks: new TaskRepository(db),
    messages: new MessageRepository(db),
    decisions: new DecisionRepository(db),
    findings: new FindingRepository(db),
    reviews: new ReviewRunRepository(db),
    evidence: new EvidenceRepository(db),
    leases: new LeaseRepository(db),
    changeSets: new ChangeSetRepository(db),
    workspaces: new WorkspaceRepository(db),
    toolCalls: new ToolCallRepository(db),
    usage: new UsageRepository(db),
    checkpoints: new CheckpointRepository(db),
    toolInvocations: new ToolInvocationRepository(db),
  }
}
