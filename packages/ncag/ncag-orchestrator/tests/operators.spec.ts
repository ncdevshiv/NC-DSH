import { existsSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { newId } from '@deepseek-ai/dsh-ncag-core'
import type { ProjectId, Task } from '@deepseek-ai/dsh-ncag-core'
import { SCRIPT_KEYS } from './scripted-model.ts'
import { startHarness } from './harness.ts'
import { readCommittedFile } from '../src/git.ts'
import type { Harness } from './harness.ts'

const harnesses: Harness[] = []

afterEach(async () => {
  while (harnesses.length > 0) {
    const harness = harnesses.pop()
    if (harness !== undefined) await harness.cleanup()
  }
})

async function harness(): Promise<Harness> {
  const created = await startHarness(new Map([[SCRIPT_KEYS.general, []]]))
  harnesses.push(created)
  return created
}

/** One ready task with a capability some registered profile serves. */
function readyTask(projectId: ProjectId, capability = 'general-implementation'): Task {
  const now = new Date().toISOString()
  return {
    id: newId('task'), projectId, requestId: null, requirementIds: [], title: 'operator task',
    description: 'd', requiredCapability: capability, assignedAgentId: null, dependencyIds: [], requiredInputs: [],
    expectedOutputs: [], acceptanceCriteria: [], requiredPermissions: [], contextRefs: [], priority: 'P2',
    deadlineMs: null, requiredReviews: [], requiredVerifications: [], status: 'IN_PROGRESS', depth: 0,
    createdAt: now, updatedAt: now, startedAt: now, completedAt: null, attempts: 0, reviewCycles: 0,
    consultation: false, statusReason: null,
  }
}

describe('ncag operator controls', { timeout: 60_000 }, () => {
  it('cancels a live agent and returns its task to the queue', async () => {
    const h = await harness()
    const project = h.orchestrator.createProject({ name: 'cancel', description: 'd', repositoryPath: h.repository })
    const work = readyTask(project.id)
    h.orchestrator.repositories.tasks.save(work)
    const profile = h.orchestrator.agents.select('general-implementation')
    const agent = h.orchestrator.agents.createInstance({
      profile, projectId: project.id, taskId: work.id, model: { provider: 'p', model: 'm' },
    })
    h.orchestrator.agents.transition(agent.id, 'INITIALIZING', 'x')
    h.orchestrator.agents.transition(agent.id, 'STARTING', 'x')
    h.orchestrator.agents.transition(agent.id, 'WORKING', 'executing')
    const cancelled = h.orchestrator.cancelAgent(agent.id, 'operator stopped it')
    expect(cancelled.status).toBe('STOPPED')
    // The work is not lost: the task returns to the queue for another attempt.
    expect(h.orchestrator.repositories.tasks.get(work.id)?.status).toBe('READY')
    expect(h.orchestrator.repositories.tasks.get(work.id)?.statusReason).toContain('operator stopped it')
    expect(h.orchestrator.store.audit.tail(20).some(entry => entry.action === 'agent-cancelled')).toBe(true)
    // A terminal agent cannot be cancelled again.
    expect(() => h.orchestrator.cancelAgent(agent.id, 'again')).toThrow(/already STOPPED/)
    expect(() => h.orchestrator.cancelAgent('agt_missing', 'x')).toThrow(/does not exist/)
  })

  it('reassigns a task to another capability and refuses an unknown one', async () => {
    const h = await harness()
    const project = h.orchestrator.createProject({ name: 'reassign', description: 'd', repositoryPath: h.repository })
    const work = readyTask(project.id)
    h.orchestrator.repositories.tasks.save(work)
    const reassigned = h.orchestrator.reassignTask(work.id, 'backend-implementation', 'needs a backend engineer')
    expect(reassigned.requiredCapability).toBe('backend-implementation')
    expect(reassigned.status).toBe('READY')
    expect(reassigned.assignedAgentId).toBeNull()
    // The permissions follow the new owner, not the previous capability.
    expect(reassigned.requiredPermissions).toContain('shell:execute')
    const restored = h.orchestrator.reassignTask(work.id, 'general-implementation', 'back to general')
    expect(restored.requiredCapability).toBe('general-implementation')
    expect(() => h.orchestrator.reassignTask(work.id, 'no-such-capability', 'x'))
      .toThrow(/no registered profile serves/)
    expect(() => h.orchestrator.reassignTask('tsk_missing', 'general-implementation', 'x')).toThrow(/does not exist/)
    const completed = { ...restored, status: 'COMPLETED' as const }
    h.orchestrator.repositories.tasks.save(completed)
    expect(() => h.orchestrator.reassignTask(work.id, 'general-implementation', 'x')).toThrow(/cannot be reassigned/)
  })

  it('applies a user decision to blocked tasks whether accepted or overridden', async () => {
    const h = await harness()
    const project = h.orchestrator.createProject({ name: 'decide', description: 'd', repositoryPath: h.repository })
    const blocked = { ...readyTask(project.id), status: 'BLOCKED' as const, statusReason: 'waiting on the decision' }
    h.orchestrator.repositories.tasks.save(blocked)
    const accepted = h.orchestrator.decisions.propose({
      projectId: project.id, taskId: null, category: 'material-architectural', author: 'orchestrator',
      problem: 'which storage', debate: null, recommendation: 'SQLite',
    })
    expect(accepted.status).toBe('AWAITING_USER')
    const resolved = h.orchestrator.resolveDecision(accepted.id, 'SQLite', 'the smaller option')
    expect(resolved.status).toBe('ACCEPTED')
    // Accepting the recommendation also unblocks the work, not only an override.
    expect(h.orchestrator.repositories.tasks.get(blocked.id)?.status).toBe('READY')
    const message = h.orchestrator.communication.history(project.id)
      .find(entry => entry.subject === 'user decision recorded')
    expect(message?.content).toContain('approved the recommendation')

    const second = h.orchestrator.decisions.propose({
      projectId: project.id, taskId: blocked.id, category: 'user-reserved', author: 'orchestrator',
      problem: 'spend more', debate: null, recommendation: 'pause',
    })
    h.orchestrator.repositories.tasks.save({ ...h.orchestrator.repositories.tasks.get(blocked.id) as Task, status: 'BLOCKED' })
    const overridden = h.orchestrator.resolveDecision(second.id, 'spend more', 'the user accepts the cost')
    expect(overridden.status).toBe('OVERRIDDEN')
    expect(overridden.rejectedRecommendation).toBe('pause')
    const constraint = h.orchestrator.project(project.id).context.constraints
    expect(constraint.some(fact => fact.text.includes('the user accepts the cost') || fact.text.includes('spend more'))).toBe(true)
    expect(h.orchestrator.repositories.tasks.get(blocked.id)?.status).toBe('READY')
  })
})
describe('ncag shared-file conflict', { timeout: 60_000 }, () => {
  it('runs an overlapping textual conflict through the real integration manager and records it', async () => {
    const h = await harness()
    h.orchestrator.createProject({ name: 'conflict', description: 'd', repositoryPath: h.repository, checks: { 'unit-test': 'exit 0' } })
    const { createWorkspace, submitChange, integrateChange } = await import('../src/git.ts')
    // Two isolated worktrees branch from the same base and edit one line in
    // opposite directions, which git can only report as a textual conflict.
    const first = createWorkspace(h.repository, join(h.repository, '.ncag-worktrees', 'conflict-a'), 'ncag/conflict-a', 'main')
    const second = createWorkspace(h.repository, join(h.repository, '.ncag-worktrees', 'conflict-b'), 'ncag/conflict-b', 'main')
    writeFileSync(join(first.path, 'README.md'), '# scratch project\n' + 'edited by A\n')
    writeFileSync(join(second.path, 'README.md'), '# scratch project\n' + 'edited by B\n')
    const submissionA = submitChange(first.path, first.baseCommit, 'A edits the shared file')
    const submissionB = submitChange(second.path, second.baseCommit, 'B edits the shared file')
    expect(submissionA.changedFiles).toEqual(['README.md'])
    expect(submissionB.changedFiles).toEqual(['README.md'])
    const merged = integrateChange(h.repository, 'main', submissionA, () => null)
    expect(merged.status).toBe('MERGED')
    const conflicted = integrateChange(h.repository, 'main', submissionB, () => null)
    expect(conflicted.status).toBe('CONFLICT')
    expect(conflicted.commit).toBeNull()
    expect(conflicted.report.some(line => line.includes('textual conflict'))).toBe(true)
    // The aborted merge left the base branch exactly as the first change set
    // integrated it, and git reports no merge in progress.
    const head = readCommittedFile(h.repository, 'README.md')
    expect(head).toContain('edited by A')
    expect(head).not.toContain('edited by B')
    expect(existsSync(join(h.repository, '.git', 'MERGE_HEAD'))).toBe(false)
  })

  it('records the conflicting change set as CONFLICT and keeps the delivered one MERGED', async () => {
    const h = await harness()
    const project = h.orchestrator.createProject({ name: 'conflict-records', description: 'd', repositoryPath: h.repository, checks: { 'unit-test': 'exit 0' } })
    const first = readyTask(project.id)
    const second = readyTask(project.id)
    h.orchestrator.repositories.tasks.save(first)
    h.orchestrator.repositories.tasks.save(second)
    h.orchestrator.repositories.changeSets.save({
      id: newId('changeset'), projectId: project.id, taskId: first.id, agentId: newId('agent'),
      workspacePath: '/tmp/a', branch: 'ncag/a', baseCommit: 'a', headCommit: 'b', changedFiles: ['README.md'],
      message: 'first', status: 'MERGED', submittedAt: new Date().toISOString(), report: [], integratedCommit: null,
    })
    h.orchestrator.repositories.changeSets.save({
      id: newId('changeset'), projectId: project.id, taskId: second.id, agentId: newId('agent'),
      workspacePath: '/tmp/b', branch: 'ncag/b', baseCommit: 'a', headCommit: 'c', changedFiles: ['README.md'],
      message: 'second', status: 'CONFLICT', submittedAt: new Date().toISOString(),
      report: ['textual conflict on README.md'], integratedCommit: null,
    })
    const listed = h.orchestrator.repositories.changeSets.listForProject(project.id)
    expect(listed.map(change => change.status).sort()).toEqual(['CONFLICT', 'MERGED'])
    const events = h.orchestrator.store.recentEvents(50, project.id)
    expect(events.length).toBeGreaterThan(0)
  })
})
