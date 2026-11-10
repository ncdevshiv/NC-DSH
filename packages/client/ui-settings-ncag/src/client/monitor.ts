/**
 * NC-AG monitor state: one polled snapshot over the `ncag` Remote namespace,
 * the selected project's decisions, and the operator actions the page exposes.
 * The controller owns no platform behavior; every read and write is a Remote
 * call into the host-plane orchestrator.
 */

import type { Context as ClientContext } from '@deepseek-ai/cordis'
import type { RemoteFailure } from '@deepseek-ai/dsh-typert-protocol'
import { createSnapshotStore, type SnapshotStore } from '@deepseek-ai/dsh-client-store'
import type {
  NcagDecision, NcagProfileView, NcagSnapshotView, NcagTraceView,
} from '@deepseek-ai/dsh-api-ncag-controller/types'

/** Poll interval while the page is mounted, in milliseconds. */
const POLL_MS = 3_000

/** Monitor lifecycle: what the page can currently render. */
export type NcagMonitorStatus = 'idle' | 'loading' | 'ready' | 'unavailable' | 'error'

/** Everything the NC-AG page renders. */
export interface NcagMonitorState {
  status: NcagMonitorStatus
  /** Latest read or write failure, cleared by the next success. */
  error: string | null
  snapshot: NcagSnapshotView | undefined
  profiles: readonly NcagProfileView[]
  decisions: readonly NcagDecision[]
  /** Project the detail panel renders. */
  selectedProjectId: string | undefined
  /** Correlation trace opened from the timeline. */
  trace: NcagTraceView | undefined
  traceError: string | null
  /** Action key currently in flight, or null. */
  busy: string | null
}

/** Outcome of one operator action, as the page reports it. */
export type NcagActionOutcome = 'accepted' | 'failed'

/**
 * Drive the NC-AG page: poll the platform snapshot, load the selected
 * project's decisions, and expose the operator controls.
 */
export class NcagMonitorController {
  readonly store: SnapshotStore<NcagMonitorState>
  private timer: ReturnType<typeof setInterval> | undefined
  private disposed = false
  private inFlight = false

  /** @param ctx - browser plugin context carrying `ctx.remote.ncag`. */
  constructor(private readonly ctx: ClientContext) {
    this.store = createSnapshotStore<NcagMonitorState>({
      status: 'idle',
      error: null,
      snapshot: undefined,
      profiles: [],
      decisions: [],
      selectedProjectId: undefined,
      trace: undefined,
      traceError: null,
      busy: null,
    })
  }

  /**
   * Start polling. A first read runs immediately so the page never shows a
   * stale empty state behind a spinner.
   */
  start(): void {
    if (this.timer !== undefined) return
    void this.refresh()
    this.timer = setInterval(() => { void this.refresh() }, POLL_MS)
  }

  /** Stop polling and abandon in-flight state publication. */
  dispose(): void {
    this.disposed = true
    if (this.timer !== undefined) clearInterval(this.timer)
    this.timer = undefined
  }

  /**
   * Select the project whose decisions and trace the detail panel renders.
   * @param projectId - project identity, or undefined for none.
   */
  selectProject(projectId: string | undefined): void {
    this.store.update((draft) => {
      draft.selectedProjectId = projectId
      draft.trace = undefined
      draft.traceError = null
    })
    void this.loadDecisions()
  }

  /** Read the platform snapshot; a failed read reports rather than replacing a good one. */
  async refresh(): Promise<void> {
    if (this.disposed || this.inFlight) return
    this.inFlight = true
    const remote = this.namespace()
    if (remote === undefined) {
      this.store.update((draft) => {
        draft.status = 'unavailable'
        draft.error = null
      })
      this.inFlight = false
      return
    }
    this.store.update((draft) => {
      if (draft.status !== 'ready') draft.status = 'loading'
    })
    const response = await remote.snapshot({})
    this.inFlight = false
    if (this.disposed) return
    if (!response.ok) {
      this.store.update((draft) => {
        draft.status = draft.snapshot === undefined ? 'error' : 'ready'
        draft.error = response.error.message
      })
      return
    }
    this.store.update((draft) => {
      draft.status = 'ready'
      draft.error = null
      draft.snapshot = response.value
    })
    void this.loadProfiles()
    void this.loadDecisions()
  }

  /**
   * Read one correlation's trace into the page.
   * @param correlationId - correlation identity from a timeline row.
   * @returns the read outcome.
   */
  async loadTrace(correlationId: string): Promise<NcagActionOutcome> {
    const projectId = this.store.getSnapshot().selectedProjectId
    const remote = this.namespace()
    if (projectId === undefined || remote === undefined) return 'failed'
    const response = await remote.trace({ projectId, correlationId })
    if (this.disposed) return 'failed'
    if (!response.ok) {
      this.store.update((draft) => {
        draft.trace = undefined
        draft.traceError = response.error.message
      })
      return 'failed'
    }
    this.store.update((draft) => {
      draft.trace = response.value
      draft.traceError = null
    })
    return 'accepted'
  }

  /** Close the correlation trace panel. */
  closeTrace(): void {
    this.store.update((draft) => {
      draft.trace = undefined
      draft.traceError = null
    })
  }

  /**
   * Create one project.
   * @param input - name, description, and optional repository path.
   * @returns the action outcome.
   */
  async createProject(input: { name: string; description: string; repositoryPath?: string }): Promise<NcagActionOutcome> {
    const outcome = await this.action('createProject', remote => remote.createProject({
      name: input.name,
      description: input.description,
      ...input.repositoryPath === undefined || input.repositoryPath.trim() === ''
        ? {}
        : { repositoryPath: input.repositoryPath },
    }))
    if (outcome === 'accepted') await this.refresh()
    return outcome
  }

  /**
   * Submit one work request to the selected project.
   * @param text - the request.
   * @returns the action outcome.
   */
  async submitRequest(text: string): Promise<NcagActionOutcome> {
    const projectId = this.store.getSnapshot().selectedProjectId
    if (projectId === undefined) return 'failed'
    const outcome = await this.action('submitRequest', remote => remote.submitRequest({ projectId, text }))
    if (outcome === 'accepted') await this.refresh()
    return outcome
  }

  /**
   * Pause or resume one project.
   * @param projectId - target project.
   * @param paused - whether the project should end up paused.
   * @returns the action outcome.
   */
  async setPaused(projectId: string, paused: boolean): Promise<NcagActionOutcome> {
    const outcome = await this.action(paused ? 'pause' : 'resume', remote => paused
      ? remote.pauseProject({ projectId })
      : remote.resumeProject({ projectId }))
    if (outcome === 'accepted') await this.refresh()
    return outcome
  }

  /**
   * Cancel one task.
   * @param taskId - target task.
   * @returns the action outcome.
   */
  async cancelTask(taskId: string): Promise<NcagActionOutcome> {
    const outcome = await this.action(`cancel-task:${taskId}`, remote => remote.cancelTask({ taskId }))
    if (outcome === 'accepted') await this.refresh()
    return outcome
  }

  /**
   * Reassign one task to a capability.
   * @param taskId - target task.
   * @param capability - required capability of the successor agent.
   * @returns the action outcome.
   */
  async reassignTask(taskId: string, capability: string): Promise<NcagActionOutcome> {
    const outcome = await this.action(`reassign-task:${taskId}`, remote => remote.reassignTask({ taskId, capability }))
    if (outcome === 'accepted') await this.refresh()
    return outcome
  }

  /**
   * Cancel one agent.
   * @param agentId - target agent.
   * @returns the action outcome.
   */
  async cancelAgent(agentId: string): Promise<NcagActionOutcome> {
    const outcome = await this.action(`cancel-agent:${agentId}`, remote => remote.cancelAgent({ agentId }))
    if (outcome === 'accepted') await this.refresh()
    return outcome
  }

  /**
   * Deliver an instruction to one agent.
   * @param agentId - target agent.
   * @param text - instruction text.
   * @returns the action outcome.
   */
  async instructAgent(agentId: string, text: string): Promise<NcagActionOutcome> {
    const outcome = await this.action(`instruct-agent:${agentId}`, remote => remote.instructAgent({ agentId, text }))
    if (outcome === 'accepted') await this.refresh()
    return outcome
  }

  /**
   * Answer one pending decision.
   * @param decisionId - target decision.
   * @param outcome - the chosen approach.
   * @returns the action outcome.
   */
  async resolveDecision(decisionId: string, outcome: string): Promise<NcagActionOutcome> {
    const result = await this.action(`decide:${decisionId}`, remote => remote.resolveDecision({ decisionId, outcome }))
    if (result === 'accepted') await this.refresh()
    return result
  }

  /** Read the registered profile summaries. */
  private async loadProfiles(): Promise<void> {
    const remote = this.namespace()
    if (remote === undefined) return
    const response = await remote.profiles()
    if (this.disposed || !response.ok) return
    this.store.update((draft) => { draft.profiles = response.value })
  }

  /** Read the selected project's decisions, or every pending decision when none is selected. */
  private async loadDecisions(): Promise<void> {
    const remote = this.namespace()
    if (remote === undefined) return
    const projectId = this.store.getSnapshot().selectedProjectId
    const response = await remote.decisions(projectId === undefined ? {} : { projectId })
    if (this.disposed || !response.ok) return
    this.store.update((draft) => { draft.decisions = response.value })
  }

  /** Run one operator action under the busy marker and report its failure. */
  private async action(
    key: string,
    run: (remote: NcagRemoteFace) => Promise<{ readonly ok: true } | { readonly ok: false; readonly error: RemoteFailure }>,
  ): Promise<NcagActionOutcome> {
    const remote = this.namespace()
    if (remote === undefined) return 'failed'
    this.store.update((draft) => { draft.busy = key })
    try {
      const response = await run(remote)
      if (this.disposed) return 'failed'
      if (!response.ok) {
        this.store.update((draft) => { draft.error = response.error.message })
        return 'failed'
      }
      this.store.update((draft) => { draft.error = null })
      return 'accepted'
    } finally {
      if (!this.disposed) this.store.update((draft) => { draft.busy = null })
    }
  }

  /** Resolve the mounted Remote namespace, absent while the plugin is not composed. */
  private namespace(): NcagRemoteFace | undefined {
    return (this.ctx.remote as unknown as { ncag?: NcagRemoteFace }).ncag
  }
}

/** The generated client face of the `ncag` namespace, as this page consumes it. */
interface NcagRemoteFace {
  snapshot(request: { projectId?: string; eventLimit?: number }): Promise<RemoteResultOf<NcagSnapshotView>>
  trace(request: { projectId: string; correlationId: string }): Promise<RemoteResultOf<NcagTraceView>>
  profiles(): Promise<RemoteResultOf<readonly NcagProfileView[]>>
  decisions(request: { projectId?: string; pendingOnly?: boolean }): Promise<RemoteResultOf<readonly NcagDecision[]>>
  createProject(request: { name: string; description: string; repositoryPath?: string }): Promise<RemoteResultOf<unknown>>
  submitRequest(request: { projectId: string; text: string }): Promise<RemoteResultOf<unknown>>
  pauseProject(request: { projectId: string }): Promise<RemoteResultOf<unknown>>
  resumeProject(request: { projectId: string }): Promise<RemoteResultOf<unknown>>
  cancelTask(request: { taskId: string }): Promise<RemoteResultOf<unknown>>
  reassignTask(request: { taskId: string; capability: string }): Promise<RemoteResultOf<unknown>>
  cancelAgent(request: { agentId: string }): Promise<RemoteResultOf<unknown>>
  instructAgent(request: { agentId: string; text: string }): Promise<RemoteResultOf<unknown>>
  resolveDecision(request: { decisionId: string; outcome: string }): Promise<RemoteResultOf<unknown>>
}

/** One Remote call's result branch. */
type RemoteResultOf<T> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: RemoteFailure }
