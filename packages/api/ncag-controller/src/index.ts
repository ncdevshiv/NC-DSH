/**
 * Host owner of the generated `ncag` Remote namespace: the browser monitor's
 * read path over the live NC-AG platform, and the operator controls a human
 * drives from the Settings NC-AG page. Every method delegates to the
 * orchestrator the host-plane `ctx.ncag` service owns — this package adds the
 * wire projection and error translation, no platform behavior.
 * @module @deepseek-ai/dsh-api-ncag-controller
 */

import { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {} from '@deepseek-ai/dsh-ncag-service'
import { Remote, RemoteError, TypertRemoteService } from '@deepseek-ai/dsh-typert-protocol'
import type {
  NcagAgent, NcagAgentCancelRequest, NcagAgentInstructRequest, NcagControlValue, NcagCreateProjectRequest,
  NcagDecision, NcagDecisionResolveRequest, NcagDecisionsRequest, NcagEventView, NcagEventsRequest,
  NcagProfileView, NcagProject, NcagProjectControlRequest, NcagRouteView, NcagSnapshotRequest, NcagSnapshotView,
  NcagSubmitRequest, NcagSubmitValue, NcagTask, NcagTaskCancelRequest, NcagTaskReassignRequest,
  NcagTraceRequest, NcagTraceView,
} from './types.ts'

export type * from './types.ts'

declare module '@deepseek-ai/cordis' {
  interface Context {
    /** Host owner of the browser NC-AG monitor Remote namespace. */
    ncagController: NcagController
  }
}

/** Default ceiling for one timeline read. */
const DEFAULT_MAX_EVENTS = 500

/** Deployment policy of the monitor Remote. */
export interface Config {
  /** Largest timeline slice one read may return (default 500). */
  readonly maxEvents?: number
}

/** Addressing facts carried by a typed failure. */
interface Address {
  readonly projectId?: string
  readonly taskId?: string
  readonly agentId?: string
  readonly decisionId?: string
}

/** Host service backing the generated `ctx.remote.ncag` namespace. */
export class NcagController extends TypertRemoteService {
  static inject = ['ncag', 'typert']

  static Config: z<Config> = z.object({
    maxEvents: z.natural().min(1).default(DEFAULT_MAX_EVENTS),
  })

  private readonly maxEvents: number

  /**
   * @param ctx - Host context carrying the mounted NC-AG platform service.
   * @param config - timeline ceiling policy.
   */
  constructor(ctx: Context, config: Config) {
    super(ctx, 'ncagController', { namespace: 'ncag' })
    // schemastery has already filled the defaulted field; the assertion records
    // that resolution rather than a hidden fallback.
    this.maxEvents = (config as Required<Config>).maxEvents
  }

  /**
   * Read the platform snapshot the monitor polls: projects with their task and
   * agent rows, usage, audit-chain state, resolved routes, and the recent
   * timeline.
   * @param request - optional project filter and timeline window.
   * @returns the platform snapshot as the monitor renders it.
   * @throws {RemoteError} `ncag/invalid` when the orchestrator cannot read its state.
   */
  @Remote
  async snapshot(request: NcagSnapshotRequest): Promise<NcagSnapshotView> {
    const orchestrator = this.orchestrator()
    const snapshot = await guardAsync('snapshot', () => orchestrator.snapshot())
    const projects = snapshot.projects
      .filter(entry => request.projectId === undefined || entry.project.id === request.projectId)
      .map(entry => ({
        project: entry.project,
        requests: entry.requests,
        tasks: entry.tasks,
        agents: entry.agents,
        decisionsPending: entry.decisionsPending,
        leases: entry.leases,
        costUsd: entry.costUsd,
      }))
    const events = request.projectId === undefined && request.eventLimit === undefined
      ? snapshot.events
      : orchestrator.store.recentEvents(clamp(request.eventLimit ?? 60, 1, this.maxEvents), request.projectId)
    return {
      generatedAt: snapshot.generatedAt,
      projects,
      usage: snapshot.usage,
      events: events.map(eventView),
      audit: { records: snapshot.audit.records, brokenAt: snapshot.audit.brokenAt },
      routes: snapshot.routes.map(routeView),
      profiles: snapshot.profiles,
      liveAgents: orchestrator.agents.listLive().length,
      pendingDecisions: orchestrator.decisions.pending().length,
    }
  }

  /**
   * Read one correlation's end-to-end trace: audit records, platform events,
   * messages, tool executions, and the referenced tasks and agents.
   * @param request - project and correlation identity.
   * @returns the trace projection.
   * @throws {RemoteError} `ncag/not-found` for an unknown project or correlation.
   */
  @Remote
  trace(request: NcagTraceRequest): NcagTraceView {
    const orchestrator = this.orchestrator()
    requireProject(orchestrator, request.projectId)
    const trace = guard(
      'trace',
      () => orchestrator.trace(request.projectId, request.correlationId),
      { projectId: request.projectId },
    )
    return {
      projectId: trace.projectId,
      correlationId: String(trace.correlationId),
      audit: trace.audit.map(entry => ({
        sequence: entry.sequence,
        at: entry.at,
        category: String(entry.category),
        action: entry.action,
        actor: entry.actor,
        projectId: entry.projectId,
        taskId: entry.taskId,
        agentId: entry.agentId,
      })),
      events: trace.events.map(eventView),
      messages: trace.messages,
      toolCalls: trace.toolCalls.map(call => ({
        sequence: call.sequence,
        at: call.at,
        agentId: call.agentId,
        taskId: call.taskId,
        tool: call.tool,
        ok: call.ok,
        error: call.error,
        output: call.output,
        durationMs: call.durationMs,
      })),
      tasks: trace.tasks,
      agents: trace.agents,
    }
  }

  /**
   * Read the newest platform events, globally or for one project.
   * @param request - optional project filter and event count.
   * @returns newest-last events.
   * @throws {RemoteError} `ncag/not-found` for an unknown project.
   */
  @Remote('recentEvents')
  recentEvents(request: NcagEventsRequest): readonly NcagEventView[] {
    const orchestrator = this.orchestrator()
    if (request.projectId !== undefined) requireProject(orchestrator, request.projectId)
    return orchestrator.store
      .recentEvents(clamp(request.limit ?? 50, 1, this.maxEvents), request.projectId)
      .map(eventView)
  }

  /**
   * Read pending decisions, or one project's decision history.
   * @param request - project and pending-only selection.
   * @returns the decision records.
   * @throws {RemoteError} `ncag/not-found` for an unknown project.
   */
  @Remote
  decisions(request: NcagDecisionsRequest): readonly NcagDecision[] {
    const orchestrator = this.orchestrator()
    if (request.projectId === undefined) return orchestrator.decisions.pending()
    requireProject(orchestrator, request.projectId)
    return orchestrator.decisions.history(request.projectId)
  }

  /**
   * Read the registered agent profiles and the capabilities they serve.
   * @returns one summary row per profile.
   */
  @Remote
  profiles(): readonly NcagProfileView[] {
    return this.orchestrator().agents.summarize()
  }

  /**
   * Create one project, anchored at the first registered workspace when no
   * repository path is supplied.
   * @param request - name, description, optional repository and instructions.
   * @returns the created project.
   * @throws {RemoteError} `ncag/invalid` when no workspace exists or the repository is unusable.
   */
  @Remote('createProject')
  createProject(request: NcagCreateProjectRequest): NcagProject {
    const service = this.ctx.ncag
    const repositoryPath = request.repositoryPath
    if (repositoryPath === undefined || repositoryPath.trim() === '') {
      return guard('createProject', () => service.createProjectFromWorkspace(request.name, request.description))
    }
    return guard('createProject', () => this.orchestrator().createProject({
      name: request.name,
      description: request.description,
      repositoryPath,
      ...request.instructions === undefined ? {} : { instructions: request.instructions },
    }))
  }

  /**
   * Submit one work request: the platform analyzes it, plans the task graph and
   * starts dispatching.
   * @param request - target project and request text.
   * @returns the planned tasks.
   * @throws {RemoteError} `ncag/not-found` for an unknown project.
   */
  @Remote('submitRequest')
  async submitRequest(request: NcagSubmitRequest): Promise<NcagSubmitValue> {
    const orchestrator = this.orchestrator()
    requireProject(orchestrator, request.projectId)
    const result = await guardAsync('submitRequest', () => orchestrator.submitRequest({
      projectId: request.projectId,
      rawText: request.text,
    }), { projectId: request.projectId })
    return { projectId: result.request.projectId, tasks: result.tasks }
  }

  /**
   * Pause one project so no further dispatch starts.
   * @param request - project and operator reason.
   * @returns the updated project.
   * @throws {RemoteError} `ncag/not-found` for an unknown project.
   */
  @Remote('pauseProject')
  pauseProject(request: NcagProjectControlRequest): NcagProject {
    const orchestrator = this.orchestrator()
    requireProject(orchestrator, request.projectId)
    return guard(
      'pauseProject',
      () => orchestrator.pauseProject(request.projectId, request.reason ?? 'paused by the user'),
      { projectId: request.projectId },
    )
  }

  /**
   * Resume one paused project and let dispatch continue.
   * @param request - project and operator reason.
   * @returns the updated project.
   * @throws {RemoteError} `ncag/not-found` for an unknown project.
   */
  @Remote('resumeProject')
  resumeProject(request: NcagProjectControlRequest): Promise<NcagProject> {
    const orchestrator = this.orchestrator()
    requireProject(orchestrator, request.projectId)
    return guardAsync(
      'resumeProject',
      () => orchestrator.resumeProject(request.projectId, request.reason ?? 'resumed by the user'),
      { projectId: request.projectId },
    )
  }

  /**
   * Cancel one task.
   * @param request - task and operator reason.
   * @returns the updated task.
   * @throws {RemoteError} `ncag/not-found` for an unknown task.
   */
  @Remote('cancelTask')
  cancelTask(request: NcagTaskCancelRequest): NcagTask {
    return guard(
      'cancelTask',
      () => this.orchestrator().cancelTask(request.taskId, request.reason ?? 'cancelled by the user'),
      { taskId: request.taskId },
    )
  }

  /**
   * Reassign one task to a successor agent holding the requested capability.
   * @param request - task, capability, and operator reason.
   * @returns the updated task.
   * @throws {RemoteError} `ncag/not-found` for an unknown task, `ncag/invalid` when no profile serves the capability.
   */
  @Remote('reassignTask')
  reassignTask(request: NcagTaskReassignRequest): NcagTask {
    return guard(
      'reassignTask',
      () => this.orchestrator().reassignTask(request.taskId, request.capability, request.reason ?? 'reassigned by the user'),
      { taskId: request.taskId },
    )
  }

  /**
   * Cancel one live agent.
   * @param request - agent and operator reason.
   * @returns the updated agent instance.
   * @throws {RemoteError} `ncag/not-found` for an unknown agent.
   */
  @Remote('cancelAgent')
  cancelAgent(request: NcagAgentCancelRequest): NcagAgent {
    return guard(
      'cancelAgent',
      () => this.orchestrator().cancelAgent(request.agentId, request.reason ?? 'cancelled by the user'),
      { agentId: request.agentId },
    )
  }

  /**
   * Deliver a human instruction to one live agent.
   * @param request - agent and instruction text.
   * @returns an acknowledgement that the message was accepted.
   * @throws {RemoteError} `ncag/not-found` for an unknown agent.
   */
  @Remote('instructAgent')
  instructAgent(request: NcagAgentInstructRequest): NcagControlValue {
    guard(
      'instructAgent',
      () => this.orchestrator().instructAgent(request.agentId, request.text),
      { agentId: request.agentId },
    )
    return { accepted: true }
  }

  /**
   * Resolve one pending decision on a human's behalf; the orchestrator records
   * the outcome permanently and keeps a rejected recommendation's consequences.
   * @param request - decision, chosen outcome, and optional rationale.
   * @returns the updated decision record.
   * @throws {RemoteError} `ncag/not-found` for an unknown decision.
   */
  @Remote('resolveDecision')
  resolveDecision(request: NcagDecisionResolveRequest): NcagDecision {
    return guard(
      'resolveDecision',
      () => this.orchestrator().resolveDecision(request.decisionId, request.outcome, request.rationale ?? null),
      { decisionId: request.decisionId },
    )
  }

  /** Resolve the mounted platform orchestrator or fail loud. */
  private orchestrator(): Context['ncag']['orchestrator'] {
    return this.ctx.ncag.orchestrator
  }
}

/** Reject an unknown project before an operation reports a misleading empty result. */
function requireProject(orchestrator: Context['ncag']['orchestrator'], projectId: string): void {
  guard('project', () => orchestrator.project(projectId), { projectId })
}

/** One event row as the monitor renders it. */
function eventView(event: {
  readonly sequence: number
  readonly at: string
  readonly kind: string
  readonly projectId: string | null
  readonly taskId: string | null
  readonly agentId: string | null
  readonly summary: string
}): NcagEventView {
  return {
    sequence: event.sequence,
    at: event.at,
    kind: event.kind,
    projectId: event.projectId,
    taskId: event.taskId,
    agentId: event.agentId,
    summary: event.summary,
  }
}

/** One resolved route as the monitor renders it. */
function routeView(route: {
  readonly provider: string
  readonly model: string
  readonly tier: 'reasoning' | 'standard' | 'fast'
  readonly inputPerMillion: number | null
  readonly outputPerMillion: number | null
}): NcagRouteView {
  return {
    provider: route.provider,
    model: route.model,
    tier: route.tier,
    inputPerMillion: route.inputPerMillion,
    outputPerMillion: route.outputPerMillion,
  }
}

/** Clamp one caller-supplied event count into the deployment's accepted window. */
function clamp(value: number, min: number, max: number): number {
  const truncated = Number.isFinite(value) ? Math.trunc(value) : min
  return Math.min(max, Math.max(min, truncated))
}

/** Whether one orchestrator diagnostic names an addressed record that no longer exists. */
function missingRecord(message: string): boolean {
  return /\bdoes not exist\b|\bnot found\b|\bunknown\b|\bno .* registered\b|\bno .* serves\b/u.test(message)
}

/**
 * Run one synchronous orchestrator call, translating its failure into the wire taxonomy.
 * @param operation - operation name carried in the error details.
 * @param run - the orchestrator call.
 * @param details - addressing facts for a not-found outcome.
 * @returns the call's value.
 * @throws {RemoteError} `ncag/not-found` for a missing addressed record, otherwise `ncag/invalid`.
 */
function guard<T>(operation: string, run: () => T, details: Address = {}): T {
  try {
    return run()
  } catch (error) {
    throw translated(operation, error, details)
  }
}

/**
 * Run one asynchronous orchestrator call, translating its rejection into the wire taxonomy.
 * @param operation - operation name carried in the error details.
 * @param run - the orchestrator call.
 * @param details - addressing facts for a not-found outcome.
 * @returns the call's settlement.
 * @throws {RemoteError} `ncag/not-found` for a missing addressed record, otherwise `ncag/invalid`.
 */
async function guardAsync<T>(operation: string, run: () => Promise<T>, details: Address = {}): Promise<T> {
  try {
    return await run()
  } catch (error) {
    throw translated(operation, error, details)
  }
}

/** Project one caught orchestrator failure onto the declared Remote failure codes. */
function translated(operation: string, error: unknown, details: Address): RemoteError {
  if (error instanceof RemoteError) return error
  const message = error instanceof Error ? error.message : String(error)
  return missingRecord(message)
    ? new RemoteError('ncag/not-found', message, details, { cause: error })
    : new RemoteError('ncag/invalid', message, { operation }, { cause: error })
}

export default NcagController
