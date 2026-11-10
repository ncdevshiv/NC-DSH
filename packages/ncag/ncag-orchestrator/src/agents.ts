/**
 * Profiles, agent instances, lifecycle enforcement and independent health
 * observation.
 *
 * The lifecycle machine in `ncag-core` decides which transitions exist; this
 * module is the only writer of agent status, so no caller can move an agent
 * into a state the machine does not permit.
 * @module @deepseek-ai/dsh-ncag-orchestrator/agents
 */

import { newId, transitionAgent, validateProfileSeed } from '@deepseek-ai/dsh-ncag-core'
import type {
  AgentInstance, AgentProfile, AgentStatus, ModelRoute, ProfileSeed, ProjectId, TaskId,
} from '@deepseek-ai/dsh-ncag-core'
import type { Repositories } from '@deepseek-ai/dsh-ncag-store'

/**
 * Timestamp strictly after one previously recorded timestamp.
 *
 * Health supervision compares recorded activity against the wall clock, so a
 * recorded timestamp must never stand still or move backwards: two events in
 * the same millisecond and a system clock correction both violate that.
 * @param previous - timestamp already stored for the agent.
 * @param now - current instant.
 * @returns an ISO timestamp later than `previous`.
 */
function advanceTimestamp(previous: string, now: Date): string {
  const current = now.toISOString()
  if (current > previous) return current
  return new Date(Date.parse(previous) + 1).toISOString()
}

/** One registered profile, with the capabilities planning matches. */
export interface ProfileSummary {
  readonly id: string
  readonly key: string
  readonly role: string
  readonly capabilities: readonly string[]
  readonly modelTier: AgentProfile['defaultTier']
}

/**
 * Profile registry and agent instance manager over the platform store.
 */
export class AgentRegistry {
  private readonly repositories: Repositories
  private readonly profiles: Map<string, AgentProfile>

  /**
   * @param repositories - store repositories owning agent and profile rows.
   */
  constructor(repositories: Repositories) {
    this.repositories = repositories
    this.profiles = new Map()
  }

  /**
   * Register one profile.
   * @param profile - the profile to register.
   * @throws Error when the key collides or the profile is invalid.
   */
  register(profile: AgentProfile): void {
    if (profile.builtIn) {
      const existing = this.profiles.get(profile.key)
      if (existing !== undefined && existing.id !== profile.id) {
        throw new Error(`ncag: profile key ${JSON.stringify(profile.key)} is already registered`)
      }
    } else {
      validateProfileSeed(profile, [...this.profiles.keys()])
    }
    this.profiles.set(profile.key, profile)
    this.repositories.profiles.save(profile)
  }

  /**
   * Create and register a new specialization.
   * @param seed - the authored profile.
   * @returns the registered profile.
   */
  create(seed: ProfileSeed): AgentProfile {
    const existing = this.profiles.get(seed.key)
    if (existing !== undefined) return existing
    validateProfileSeed(seed, [...this.profiles.keys()])
    const profile: AgentProfile = {
      ...seed,
      id: newId('profile'),
      builtIn: false,
    }
    this.profiles.set(profile.key, profile)
    this.repositories.profiles.save(profile)
    return profile
  }

  /**
   * Load every profile the store holds, used at startup.
   * @param builtIns - the built-in profiles the deployment ships.
   */
  load(builtIns: readonly AgentProfile[]): void {
    for (const profile of builtIns) this.register(profile)
    for (const stored of this.repositories.profiles.list()) {
      if (stored.builtIn) continue
      this.profiles.set(stored.key, stored)
    }
  }

  /**
   * Read one profile by key.
   * @param key - registry key.
   * @returns the profile, or null when unregistered.
   */
  get(key: string): AgentProfile | null {
    return this.profiles.get(key) ?? null
  }

  /**
   * Read every registered profile.
   * @returns the profiles in registration order.
   */
  list(): AgentProfile[] {
    return [...this.profiles.values()]
  }

  /**
   * Capabilities at least one profile can serve.
   * @returns the capability tags.
   */
  capabilities(): string[] {
    const set = new Set<string>()
    for (const profile of this.profiles.values()) {
      for (const capability of profile.capabilities) set.add(capability)
    }
    return [...set].sort()
  }

  /**
   * Select the profile that will execute one capability.
   * @param capability - required capability.
   * @returns the selected profile.
   * @throws Error when no registered profile serves the capability.
   */
  select(capability: string): AgentProfile {
    const matches = this.list().filter(profile => profile.capabilities.includes(capability))
    const chosen = matches[0]
    if (chosen === undefined) {
      throw new Error(`ncag: no registered profile serves capability ${JSON.stringify(capability)}; register one before assigning the task`)
    }
    return chosen
  }

  /**
   * Summarize the registry for the dashboard.
   * @returns one row per profile.
   */
  summarize(): ProfileSummary[] {
    return this.list().map(profile => ({
      id: profile.id,
      key: profile.key,
      role: profile.role,
      capabilities: profile.capabilities,
      modelTier: profile.defaultTier,
    }))
  }

  /**
   * Create one agent instance in the CREATED state.
   * @param request - profile, project, task and model route.
   * @returns the created instance.
   */
  createInstance(request: {
    readonly profile: AgentProfile
    readonly projectId: ProjectId
    readonly taskId: TaskId | null
    readonly model: ModelRoute
  }): AgentInstance {
    const now = new Date().toISOString()
    const instance: AgentInstance = {
      id: newId('agent'),
      profileId: request.profile.id,
      profileKey: request.profile.key,
      projectId: request.projectId,
      taskId: request.taskId,
      status: 'CREATED',
      createdAt: now,
      updatedAt: now,
      lastHeartbeatAt: now,
      lastActivityAt: now,
      workspacePath: null,
      branch: null,
      model: request.model,
      capabilities: [...request.profile.capabilities],
      usage: { inputTokens: 0, outputTokens: 0, cachedTokens: 0, costUsd: 0, calls: 0 },
      failureCount: 0,
      statusReason: null,
      activeCorrelationId: null,
      steps: 0,
    }
    this.repositories.agents.save(instance)
    return instance
  }

  /**
   * Move one agent to a new lifecycle state.
   * @param agentId - agent identifier.
   * @param to - requested state.
   * @param reason - why the state changed.
   * @returns the updated instance.
   * @throws Error when the agent does not exist or the transition is not permitted.
   */
  transition(agentId: string, to: AgentStatus, reason: string | null = null): AgentInstance {
    const current = this.repositories.agents.get(agentId)
    if (current === null) throw new Error(`ncag: agent ${agentId} does not exist`)
    const next = transitionAgent(current.status, to)
    const now = advanceTimestamp(current.updatedAt, new Date())
    const updated: AgentInstance = {
      ...current,
      status: next,
      statusReason: reason,
      updatedAt: now,
      lastHeartbeatAt: now,
      failureCount: next === 'FAILED' ? current.failureCount + 1 : current.failureCount,
    }
    this.repositories.agents.save(updated)
    return updated
  }

  /**
   * Record a heartbeat and activity observation.
   * @param agentId - agent identifier.
   * @param producedProgress - whether the agent produced durable progress since the last heartbeat.
   * @returns the updated instance.
   */
  heartbeat(agentId: string, producedProgress: boolean): AgentInstance {
    const current = this.repositories.agents.get(agentId)
    if (current === null) throw new Error(`ncag: agent ${agentId} does not exist`)
    const now = advanceTimestamp(current.lastHeartbeatAt, new Date())
    const updated: AgentInstance = {
      ...current,
      lastHeartbeatAt: now,
      lastActivityAt: producedProgress ? now : current.lastActivityAt,
      updatedAt: now,
    }
    this.repositories.agents.save(updated)
    return updated
  }

  /**
   * Attach a workspace and model route to one agent.
   * @param agentId - agent identifier.
   * @param workspacePath - absolute worktree path.
   * @param branch - branch the worktree tracks.
   * @returns the updated instance.
   */
  attachWorkspace(agentId: string, workspacePath: string, branch: string): AgentInstance {
    const current = this.repositories.agents.get(agentId)
    if (current === null) throw new Error(`ncag: agent ${agentId} does not exist`)
    const updated: AgentInstance = { ...current, workspacePath, branch, updatedAt: advanceTimestamp(current.updatedAt, new Date()) }
    this.repositories.agents.save(updated)
    return updated
  }

  /**
   * Read one agent.
   * @param agentId - agent identifier.
   * @returns the agent, or null when it does not exist.
   */
  getInstance(agentId: string): AgentInstance | null {
    return this.repositories.agents.get(agentId)
  }

  /**
   * Read a project's agents.
   * @param projectId - project identifier.
   * @returns the agents in creation order.
   */
  listInstances(projectId: string): AgentInstance[] {
    return this.repositories.agents.listForProject(projectId)
  }

  /**
   * Read every agent still alive across all projects.
   * @returns the live agents.
   */
  listLive(): AgentInstance[] {
    return this.repositories.agents.listLive()
  }
}
