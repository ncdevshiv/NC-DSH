/**
 * Resource ownership: exclusive write leases and shared read leases over
 * repository-relative paths, with waiting queues, renewal, expiry, forced
 * release and fencing tokens.
 *
 * A lease is the platform's only authority to write a path. An agent that
 * acts on an expired lease is refused by the fencing check, so a slow agent
 * cannot overwrite a newer owner's work.
 * @module @deepseek-ai/dsh-ncag-orchestrator/resources
 */

import { newId } from '@deepseek-ai/dsh-ncag-core'
import type { ResourceLease } from '@deepseek-ai/dsh-ncag-core'
import type { LeaseWaiter, Repositories } from '@deepseek-ai/dsh-ncag-store'

/** Configuration of the lease manager. */
export interface LeaseConfig {
  /** Default lease duration in milliseconds. */
  readonly ttlMs: number
  /** Whether two readers may hold the same path at the same time. */
  readonly allowSharedRead: boolean
}

/** One refused acquisition, carrying everything the asking agent needs to know. */
export interface LeaseConflict {
  readonly resourcePath: string
  readonly ownerAgentId: string
  readonly ownerTaskId: string | null
  readonly reason: string
  readonly expiresAtMs: number
  /** Whether another workspace could satisfy the request. */
  readonly alternativeWorkspace: string | null
}

/** Result of one acquisition attempt. */
export type AcquireResult =
  | { readonly granted: true; readonly lease: ResourceLease }
  | { readonly granted: false; readonly conflict: LeaseConflict }

/**
 * Lease manager over the platform store. All state is durable, so a restart
 * resumes with the leases that were active before it.
 */
export class LeaseManager {
  private readonly repositories: Repositories
  private readonly config: LeaseConfig
  private readonly knownChains: Set<string>

  /**
   * @param repositories - store repositories owning lease rows.
   * @param config - lease policy.
   */
  constructor(repositories: Repositories, config: LeaseConfig) {
    this.repositories = repositories
    this.config = config
    this.knownChains = new Set()
  }

  /**
   * Request a lease over one path.
   * @param request - project, path, mode, agent, task and the stated reason.
   * @returns the granted lease or the conflict that refused it.
   */
  acquire(request: { projectId: string; resourcePath: string; mode: 'read' | 'write'; agentId: string; taskId: string | null; reason: string }): AcquireResult {
    const now = Date.now()
    this.expireStale(request.projectId, now)
    const active = this.repositories.leases.active(request.projectId)
    const holders = active.filter(lease => lease.resourcePath === request.resourcePath && lease.mode === 'write')
    const sharedReaders = active.filter(lease => lease.resourcePath === request.resourcePath && lease.mode === 'read')
    const conflictWith = holders[0]
    // A writer excludes everyone. A reader is excluded by a writer always, and
    // by another reader unless shared reads are enabled.
    const refusesWrite = request.mode === 'write' && (conflictWith !== undefined || sharedReaders.length > 0)
    const refusesRead = request.mode === 'read'
      && (conflictWith !== undefined || (!this.config.allowSharedRead && sharedReaders.length > 0))
    if (refusesWrite || refusesRead) {
      const blocker = conflictWith ?? sharedReaders[0]
      /* v8 ignore next -- a refusal is always caused by a blocker, so this guard is unreachable defense. */
      if (blocker !== undefined) {
        // The refusal is durable: the waiting agent and every operator can see
        // who is queued behind this path and why it is occupied.
        this.repositories.leases.wait({
          projectId: request.projectId,
          resourcePath: request.resourcePath,
          agentId: request.agentId,
          taskId: request.taskId,
          mode: request.mode,
          reason: request.reason,
        })
        return {
          granted: false,
          conflict: {
            resourcePath: request.resourcePath,
            ownerAgentId: blocker.agentId,
            ownerTaskId: blocker.taskId,
            reason: blocker.reason,
            expiresAtMs: blocker.expiresAtMs,
            alternativeWorkspace: null,
          },
        }
      }
    }
    if (request.mode === 'read') {
      const sameOwner = sharedReaders.find(lease => lease.agentId === request.agentId)
      if (sameOwner !== undefined) return { granted: true, lease: this.renew(sameOwner) }
    }
    const fencingToken = this.nextToken(request.projectId)
    const lease: ResourceLease = {
      id: newId('lease'),
      projectId: request.projectId as ResourceLease['projectId'],
      resourcePath: request.resourcePath,
      mode: request.mode,
      agentId: request.agentId as ResourceLease['agentId'],
      taskId: request.taskId as ResourceLease['taskId'],
      fencingToken,
      acquiredAtMs: now,
      renewedAtMs: now,
      expiresAtMs: now + this.config.ttlMs,
      status: 'ACTIVE',
      reason: request.reason,
    }
    this.repositories.leases.insert(lease)
    // The grant ends this agent's wait for the path.
    this.repositories.leases.clearWait(request.projectId, request.resourcePath, request.agentId, request.mode)
    return { granted: true, lease }
  }

  /**
   * Renew one held lease.
   * @param lease - the held lease.
   * @returns the renewed lease.
   * @throws Error when the lease is no longer active or has been superseded.
   */
  renew(lease: ResourceLease): ResourceLease {
    const current = this.repositories.leases.get(lease.id)
    if (current === null || current.status !== 'ACTIVE') {
      throw new Error(`ncag: lease ${lease.id} is ${current?.status ?? 'missing'} and cannot be renewed`)
    }
    if (current.fencingToken !== lease.fencingToken) {
      throw new Error(`ncag: lease ${lease.id} was superseded by fencing token ${current.fencingToken}`)
    }
    const now = Date.now()
    const renewed: ResourceLease = {
      ...current,
      renewedAtMs: now,
      expiresAtMs: now + this.config.ttlMs,
      reason: lease.reason,
    }
    this.repositories.leases.update(renewed)
    return renewed
  }

  /**
   * Release one lease.
   * @param leaseId - lease identifier.
   */
  release(leaseId: string): void {
    const lease = this.repositories.leases.get(leaseId)
    if (lease === null || lease.status !== 'ACTIVE') return
    const released: ResourceLease = { ...lease, status: 'RELEASED' }
    this.repositories.leases.update(released)
    this.knownChains.delete(released.projectId)
  }

  /**
   * Release every lease of one agent, used when its task ends or it fails.
   * @param agentId - agent identifier.
   */
  releaseAll(agentId: string): void {
    this.repositories.leases.releaseForAgent(agentId, 'RELEASED')
  }

  /**
   * Force-release one lease during a controlled recovery.
   * @param leaseId - lease identifier.
   * @returns the released lease.
   * @throws Error when the lease does not exist.
   */
  forceRelease(leaseId: string): ResourceLease {
    const lease = this.repositories.leases.get(leaseId)
    if (lease === null) throw new Error(`ncag: lease ${leaseId} does not exist`)
    const revoked: ResourceLease = { ...lease, status: 'REVOKED' }
    this.repositories.leases.update(revoked)
    this.knownChains.delete(revoked.projectId)
    return revoked
  }

  /**
   * Verify that an acting agent still holds a live lease carrying its fencing token.
   * @param lease - the lease the agent believes it holds.
   * @returns whether the agent may write the path.
   */
  canAct(lease: ResourceLease): boolean {
    const current = this.repositories.leases.get(lease.id)
    if (current === null || current.status !== 'ACTIVE') return false
    if (current.fencingToken !== lease.fencingToken) return false
    return current.expiresAtMs > Date.now()
  }

  /**
   * Read the active leases of a project.
   * @param projectId - project identifier.
   * @returns the active leases.
   */
  active(projectId: string): ResourceLease[] {
    return this.repositories.leases.active(projectId)
  }

  /**
   * Read the agents queued behind the leases of one project.
   *
   * A waiter is listed per path and mode, so an operator can see the queue a
   * blocked agent is in and how long it has waited. A path whose last holder
   * released it keeps its queue until a grant clears it: the waiter is still
   * waiting for that path, even though nobody holds it this instant.
   * @param projectId - project identifier.
   * @returns one entry per waiting agent, path and mode.
   */
  waiting(projectId: string): LeaseWaiter[] {
    return this.repositories.leases.waitingForProject(projectId)
  }

  /** Expire the leases whose deadline passed, recording the transition. */
  private expireStale(projectId: string, now: number): void {
    for (const lease of this.repositories.leases.active(projectId)) {
      if (lease.expiresAtMs >= now) continue
      const expired: ResourceLease = { ...lease, status: 'EXPIRED' }
      this.repositories.leases.update(expired)
    }
  }

  /** Issue the next fencing token for one project, monotonically increasing. */
  private nextToken(projectId: string): number {
    return this.repositories.leases.maxFencingToken(projectId) + 1
  }
}
