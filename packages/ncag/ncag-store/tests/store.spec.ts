import { DatabaseSync } from 'node:sqlite'
import { existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import {
  EMPTY_USAGE, addUsage, createRepositories, NCAG_SCHEMA_VERSION, openPlatformDatabase, NcagStore,
} from '../src/index.ts'
import type { EventRecord } from '../src/index.ts'
import { decodeBool, decodeJson, decodeNullableNumber, encodeBool, encodeJson, asRows } from '../src/columns.ts'
import { newId, adoptId } from '@deepseek-ai/dsh-ncag-core'
import type {
  AcceptanceCriterion, AgentInstance, AgentProfile, ChangeSet, DecisionRecord, NcagMessage,
  Project, Requirement, ResourceLease, ReviewFinding, Task, UserRequest, VerificationEvidence,
  WorkspaceRecord,
} from '@deepseek-ai/dsh-ncag-core'
import { BUILT_IN_PROFILES, builtInProfile } from '@deepseek-ai/dsh-ncag-core'

const stores: NcagStore[] = []
const directories: string[] = []

afterEach(() => {
  while (stores.length > 0) stores.pop()?.close()
  while (directories.length > 0) {
    const directory = directories.pop()
    if (directory !== undefined) rmSync(directory, { recursive: true, force: true })
  }
})

function store(): NcagStore {
  const opened = NcagStore.open(':memory:')
  stores.push(opened)
  return opened
}

function project(overrides: Partial<Project> = {}): Project {
  const now = new Date().toISOString()
  return {
    id: newId('project'),
    name: 'p',
    description: 'd',
    repositoryPath: '/tmp/repo',
    baseBranch: 'main',
    status: 'ACTIVE',
    createdAt: now,
    updatedAt: now,
    autonomy: {
      requireArchitecturalApproval: true, autonomousSpawning: true, challengeOnConflict: true,
      requireVerification: true, maxCostUsd: 10, maxConcurrentAgents: 2, maxAgentsPerTask: 2,
      maxRetries: 2, maxReviewCycles: 2, maxTaskDurationMs: 1000, modelPolicy: 'balanced',
      preferredProviders: [],
    },
    instructions: '',
    context: { architecture: [], contracts: [], constraints: [], outstanding: [], knownProblems: [] },
    checks: {},
    usage: EMPTY_USAGE,
    ...overrides,
  }
}

function request(projectId: Project['id']): UserRequest {
  return {
    id: newId('request'),
    projectId,
    rawText: 'build it',
    createdAt: new Date().toISOString(),
    intent: {
      goal: 'g', motivation: 'm', outcomes: [], constraints: [], technologies: [], scope: 's',
      priority: 'P1', quality: 'q', security: [],
    },
    ambiguities: [],
    requirements: [],
    status: 'PLANNED',
  }
}

function requirement(requestId: UserRequest['id']): Requirement {
  return {
    id: newId('requirement'),
    requestId,
    description: 'r',
    category: 'functional',
    priority: 'P1',
    source: 'user',
    dependencies: [],
    acceptanceCriteria: [],
    status: 'APPROVED',
  }
}

function agent(projectId: Project['id']): AgentInstance {
  const now = new Date().toISOString()
  return {
    id: newId('agent'),
    profileId: adoptId('profile', 'prf_test'),
    profileKey: 'general-purpose',
    projectId,
    taskId: null,
    status: 'CREATED',
    createdAt: now,
    updatedAt: now,
    lastHeartbeatAt: now,
    lastActivityAt: now,
    workspacePath: null,
    branch: null,
    model: { provider: 'p', model: 'm' },
    capabilities: ['analysis'],
    usage: EMPTY_USAGE,
    failureCount: 0,
    statusReason: null,
    activeCorrelationId: null,
    steps: 0,
  }
}

function task(projectId: Project['id']): Task {
  const now = new Date().toISOString()
  return {
    id: newId('task'),
    projectId,
    requestId: null,
    requirementIds: [],
    title: 't',
    description: 'd',
    requiredCapability: 'analysis',
    assignedAgentId: null,
    dependencyIds: [],
    requiredInputs: [],
    expectedOutputs: [],
    acceptanceCriteria: [],
    requiredPermissions: [],
    contextRefs: [],
    priority: 'P1',
    deadlineMs: null,
    requiredReviews: [],
    requiredVerifications: [],
    status: 'PLANNED',
    depth: 0,
    createdAt: now,
    updatedAt: now,
    startedAt: null,
    completedAt: null,
    attempts: 0,
    reviewCycles: 0,
    consultation: false,
    statusReason: null,
  }
}

describe('ncag store columns', () => {
  it('encodes and decodes JSON, booleans and nullable numbers', () => {
    expect(encodeJson({ a: 1 })).toBe('{"a":1}')
    expect(decodeJson<{ a: number }>('{"a":1}')).toEqual({ a: 1 })
    expect(decodeJson<number>(null, 7)).toBe(7)
    expect(() => decodeJson<number>(null)).toThrow(/required JSON column is null/)
    expect(encodeBool(true)).toBe(1)
    expect(encodeBool(false)).toBe(0)
    expect(decodeBool(0)).toBe(false)
    expect(decodeBool(3)).toBe(true)
    expect(decodeNullableNumber(null)).toBeNull()
    expect(decodeNullableNumber(4)).toBe(4)
    expect(asRows<{ x: number }>([{ x: 1 }])).toEqual([{ x: 1 }])
  })

  it('sums usage totals and rounds the cost', () => {
    const total = addUsage({ inputTokens: 1, outputTokens: 2, cachedTokens: 3, costUsd: 0.1, calls: 4 },
      { inputTokens: 5, outputTokens: 6, cachedTokens: 7, costUsd: 0.2, calls: 8 })
    expect(total).toEqual({ inputTokens: 6, outputTokens: 8, cachedTokens: 10, costUsd: 0.3, calls: 12 })
  })
})

describe('ncag store schema', () => {
  it('creates a fresh database at the current version', () => {
    const db = openPlatformDatabase(':memory:')
    const { user_version: version } = db.prepare('PRAGMA user_version').get() as { user_version: number }
    expect(version).toBe(NCAG_SCHEMA_VERSION)
    // The fresh stamp records version 1 plus one history row per migration.
    expect((db.prepare('SELECT COUNT(*) AS n FROM schema_history').get() as { n: number }).n).toBe(NCAG_SCHEMA_VERSION)
    db.close()
  })

  it('migrates an older database forward through every pending migration', () => {
    const directory = mkdtempSync(join(tmpdir(), 'ncag-schema-'))
    directories.push(directory)
    const path = join(directory, 'nested', 'platform.db')
    mkdirSync(join(directory, 'nested'), { recursive: true })
    const legacy = new DatabaseSync(path, { open: true })
    legacy.exec(`CREATE TABLE projects (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL, repository_path TEXT NOT NULL,
  base_branch TEXT NOT NULL, status TEXT NOT NULL, instructions TEXT NOT NULL, autonomy TEXT NOT NULL,
  context TEXT NOT NULL, usage TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL) STRICT`)
    legacy.exec('PRAGMA user_version = 1')
    legacy.close()
    const migrated = openPlatformDatabase(path)
    const { user_version: version } = migrated.prepare('PRAGMA user_version').get() as { user_version: number }
    expect(version).toBe(NCAG_SCHEMA_VERSION)
    // Version 4 added the checks column the fresh schema already carries.
    const columns = migrated.prepare('PRAGMA table_info(projects)').all() as Array<{ name: string }>
    expect(columns.map(column => column.name)).toContain('checks')
    // Version 5 added the consultation flag to the tasks table.
    const taskColumns = migrated.prepare('PRAGMA table_info(tasks)').all() as Array<{ name: string }>
    expect(taskColumns.map(column => column.name)).toContain('consultation')
    migrated.close()

    // Opening a fresh file creates its missing parent directories.
    const freshPath = join(directory, 'fresh', 'deep', 'platform.db')
    const fresh = openPlatformDatabase(freshPath)
    fresh.close()
    expect(existsSync(freshPath)).toBe(true)
  })

  it('rolls a failed fresh stamp back and reports it', () => {
    const directory = mkdtempSync(join(tmpdir(), 'ncag-schema-stamp-fail-'))
    directories.push(directory)
    const path = join(directory, 'platform.db')
    const raw = new DatabaseSync(path, { open: true })
    raw.exec('CREATE TABLE schema_history (version INTEGER PRIMARY KEY, description TEXT NOT NULL, applied_at TEXT NOT NULL) STRICT')
    raw.exec("CREATE TRIGGER fail BEFORE INSERT ON schema_history BEGIN SELECT RAISE(ABORT, 'nope'); END")
    raw.close()
    expect(() => openPlatformDatabase(path)).toThrow()
  })

  it('rolls a failed migration back and reports it', () => {
    const directory = mkdtempSync(join(tmpdir(), 'ncag-schema-migrate-fail-'))
    directories.push(directory)
    const path = join(directory, 'platform.db')
    const raw = new DatabaseSync(path, { open: true })
    raw.exec('CREATE TABLE schema_history (version INTEGER PRIMARY KEY, description TEXT NOT NULL, applied_at TEXT NOT NULL) STRICT')
    raw.exec("CREATE TRIGGER fail BEFORE INSERT ON schema_history BEGIN SELECT RAISE(ABORT, 'nope'); END")
    raw.exec('PRAGMA user_version = 1')
    raw.close()
    expect(() => openPlatformDatabase(path)).toThrow()
  })

  it('refuses a database newer than this build', () => {
    const directory = mkdtempSync(join(tmpdir(), 'ncag-schema-newer-'))
    directories.push(directory)
    const path = join(directory, 'platform.db')
    const newer = new DatabaseSync(path, { open: true })
    newer.exec(`PRAGMA user_version = ${NCAG_SCHEMA_VERSION + 1}`)
    newer.close()
    expect(() => openPlatformDatabase(path)).toThrow(/newer than this build/)
  })
})

describe('ncag store events and audit', () => {
  it('emits events, filters by project and notifies subscribers', () => {
    const opened = store()
    const seen: EventRecord[] = []
    const dispose = opened.subscribe(event => seen.push(event))
    const first = opened.emitEvent({ kind: 'k1', summary: 's1' })
    const second = opened.emitEvent({ kind: 'k2', projectId: 'prj_x', summary: 's2', data: { a: 1 } })
    expect(first.sequence).toBeLessThan(second.sequence)
    expect(first.projectId).toBeNull()
    expect(second.data).toEqual({ a: 1 })
    expect(seen).toHaveLength(2)
    expect(opened.recentEvents(1)).toEqual([second])
    expect(opened.recentEvents(10, 'prj_x')).toEqual([second])
    dispose()
    opened.emitEvent({ kind: 'k3', summary: 's3' })
    expect(seen).toHaveLength(2)
  })

  it('appends an audit chain and forwards records as events', () => {
    const opened = store()
    const seen: EventRecord[] = []
    opened.subscribe(event => seen.push(event))
    const entry = opened.recordAudit({ category: 'task', action: 'created', actor: 'orchestrator', correlationId: newId('correlation') })
    expect(entry.sequence).toBe(1)
    expect(entry.detail).toEqual({})
    expect(seen.some(event => event.kind === 'audit/task')).toBe(true)
    expect(opened.verifyAudit()).toEqual({ records: 1, brokenAt: null })
    opened.audit.append({ category: 'agent', action: 'spawned', actor: 'a', projectId: 'prj', taskId: 'tsk', agentId: 'agt', correlationId: newId('correlation'), detail: { x: 1 } })
    expect(opened.audit.tail(1)).toHaveLength(1)
    expect(opened.audit.tail(2).map(item => item.sequence)).toEqual([1, 2])
    // One correlation exposes exactly the records recorded under it.
    const shared = newId('correlation')
    opened.audit.append({ category: 'review', action: 'approved', actor: 'o', projectId: 'prj', correlationId: shared, detail: {} })
    opened.emitEvent({ kind: 'traced', projectId: 'prj', summary: 's', data: { correlationId: shared } })
    expect(opened.audit.forCorrelation('prj', shared).map(item => item.action)).toEqual(['approved'])
    // The audit record's forwarded event carries the same identifier, so the trace includes it.
    expect(opened.eventsForCorrelation('prj', shared).map(item => item.kind)).toEqual(['audit/review', 'traced'])
    expect(opened.audit.forCorrelation('prj', newId('correlation'))).toEqual([])
    expect(opened.eventsForCorrelation('other', shared)).toEqual([])
  })

  it('detects a tampered audit record', () => {
    const opened = store()
    opened.recordAudit({ category: 'task', action: 'created', actor: 'o', correlationId: newId('correlation') })
    opened.recordAudit({ category: 'task', action: 'done', actor: 'o', correlationId: newId('correlation') })
    // Rewriting a record's content breaks verification at that record.
    opened.db.prepare('UPDATE audit SET action = ? WHERE sequence = 1').run('tampered')
    expect(opened.verifyAudit().brokenAt).toBe(1)
    // A gap in the sequence is detected too.
    opened.db.prepare('DELETE FROM audit WHERE sequence = 2').run()
    expect(opened.verifyAudit().records).toBe(1)
  })

  it('rejects a repeated close and reads the chain tip from disk', () => {
    const directory = mkdtempSync(join(tmpdir(), 'ncag-audit-'))
    directories.push(directory)
    const path = join(directory, 'platform.db')
    const first = NcagStore.open(path)
    first.recordAudit({ category: 'task', action: 'created', actor: 'o', correlationId: newId('correlation') })
    first.close()
    first.close()
    const reopened = NcagStore.open(path)
    stores.push(reopened)
    const entry = reopened.recordAudit({ category: 'task', action: 'created', actor: 'o', correlationId: newId('correlation') })
    expect(entry.sequence).toBe(2)
    expect(reopened.verifyAudit()).toEqual({ records: 2, brokenAt: null })
  })
})

describe('ncag store repositories', () => {
  it('persists and reads projects and requests', () => {
    const opened = store()
    const repositories = createRepositories(opened.db)
    const created = project()
    repositories.projects.save(created)
    expect(repositories.projects.get(created.id)).toEqual(created)
    expect(repositories.projects.get('prj_missing')).toBeNull()
    expect(repositories.projects.list()).toEqual([created])

    const req = request(created.id)
    repositories.requests.save({ ...req, ambiguities: [{ id: 'a', description: 'd', material: false, assumption: 'x', question: null }] })
    const firstRequirement = requirement(req.id)
    const secondRequirement = requirement(req.id)
    const withDependencies: Requirement = {
      ...secondRequirement,
      dependencies: [firstRequirement.id],
      acceptanceCriteria: [{ id: 'ac-1', text: 't', verification: 'command', command: 'exit 0' }],
    }
    repositories.requirements.saveAll(created.id, req.id, [firstRequirement, withDependencies])
    const read = repositories.requests.get(req.id)
    expect(read?.requirements).toHaveLength(2)
    expect(read?.requirements[1]?.dependencies).toEqual([firstRequirement.id])
    expect(read?.requirements[1]?.acceptanceCriteria).toHaveLength(1)
    expect(read?.ambiguities).toHaveLength(1)
    expect(repositories.requests.get('req_missing')).toBeNull()
    expect(repositories.requests.listForProject(created.id)).toHaveLength(1)
    // Re-saving the same set keeps it; re-saving a smaller set drops the rest.
    repositories.requirements.saveAll(created.id, req.id, [firstRequirement, withDependencies])
    expect(repositories.requests.get(req.id)?.requirements).toHaveLength(2)
    repositories.requirements.saveAll(created.id, req.id, [])
    expect(repositories.requests.get(req.id)?.requirements).toEqual([])
  })

  it('persists agents, tasks and profiles', () => {
    const opened = store()
    const repositories = createRepositories(opened.db)
    const created = project()
    repositories.projects.save(created)
    const work = task(created.id)
    repositories.tasks.save(work)
    const instance: AgentInstance = {
      ...agent(created.id),
      taskId: work.id,
      workspacePath: '/tmp/w',
      branch: 'ncag/a',
      statusReason: 'reason',
      activeCorrelationId: newId('correlation'),
      steps: 3,
      failureCount: 1,
    }
    repositories.agents.save(instance)
    expect(repositories.agents.get(instance.id)).toEqual(instance)
    expect(repositories.agents.get('agt_missing')).toBeNull()
    expect(repositories.agents.listForProject(created.id)).toHaveLength(1)
    expect(repositories.agents.listLive()).toHaveLength(1)
    repositories.agents.save({ ...instance, status: 'STOPPED' })
    expect(repositories.agents.listLive()).toHaveLength(0)
    repositories.agents.remove(instance.id)
    expect(repositories.agents.get(instance.id)).toBeNull()
    // An unassigned agent carries no task and no active correlation.
    const minimalAgent = agent(created.id)
    repositories.agents.save(minimalAgent)
    expect(repositories.agents.get(minimalAgent.id)).toEqual(minimalAgent)
    repositories.agents.remove(minimalAgent.id)

    expect(repositories.tasks.get(work.id)).toEqual(work)
    expect(repositories.tasks.get('tsk_missing')).toBeNull()
    expect(repositories.tasks.listForProject(created.id)).toHaveLength(1)
    const dependent = task(created.id)
    const detailed: Task = {
      ...dependent,
      requestId: newId('request'),
      requirementIds: [newId('requirement')],
      assignedAgentId: instance.id,
      dependencyIds: [work.id],
      deadlineMs: 1234,
      statusReason: 'blocked',
      startedAt: new Date().toISOString(),
      completedAt: new Date().toISOString(),
      attempts: 2,
      reviewCycles: 1, consultation: false,
      depth: 1,
    }
    repositories.tasks.save(detailed)
    expect(repositories.tasks.get(detailed.id)).toEqual(detailed)
    repositories.tasks.remove(detailed.id)
    repositories.tasks.remove(work.id)
    expect(repositories.tasks.get(work.id)).toBeNull()

    const seed = BUILT_IN_PROFILES.find(profile => profile.key === 'general-purpose')
    if (seed === undefined) throw new Error('missing seed')
    const profile: AgentProfile = builtInProfile(seed)
    repositories.profiles.save(profile)
    expect(repositories.profiles.get(profile.id)).toEqual(profile)
    expect(repositories.profiles.get('prf_missing')).toBeNull()
    expect(repositories.profiles.getByKey(profile.key)).toEqual(profile)
    expect(repositories.profiles.getByKey('missing')).toBeNull()
    expect(repositories.profiles.list()).toHaveLength(1)
    repositories.profiles.remove(profile.id)
    expect(repositories.profiles.list()).toHaveLength(0)
  })

  it('persists messages, acknowledgements and cursors', () => {
    const opened = store()
    const repositories = createRepositories(opened.db)
    const created = project()
    repositories.projects.save(created)
    const message: NcagMessage = {
      id: newId('message'),
      timestamp: new Date().toISOString(),
      sender: newId('agent'),
      target: { kind: 'channel', channel: 'project:p' },
      projectId: created.id,
      taskId: null,
      type: 'INFORMATION',
      subject: 's',
      content: 'c',
      contextRefs: [],
      correlationId: newId('correlation'),
      priority: 'normal',
      requiresAck: true,
      deliveryStatus: 'PENDING',
      deliveredAt: null,
      acknowledgedAt: null,
      attempts: 0,
      expiresAtMs: null,
    }
    repositories.messages.insert(message)
    expect(repositories.messages.get(message.id)).toEqual(message)
    expect(repositories.messages.get('msg_missing')).toBeNull()
    expect(repositories.messages.listForProject(created.id)).toHaveLength(1)
    expect(repositories.messages.forCorrelation(created.id, message.correlationId)).toHaveLength(1)
    expect(repositories.messages.forCorrelation(created.id, newId('correlation'))).toEqual([])
    expect(repositories.messages.pending(10)).toHaveLength(1)
    repositories.messages.updateDelivery({ ...message, deliveryStatus: 'DELIVERED', deliveredAt: new Date().toISOString(), attempts: 1 }, 'nope')
    expect(repositories.messages.pending(10)).toHaveLength(0)
    expect(repositories.messages.acknowledge(message.id, 'agt_1')).toBe(true)
    expect(repositories.messages.acknowledge(message.id, 'agt_1')).toBe(false)
    expect(repositories.messages.cursor('agt_1')).toBe(0)
    repositories.messages.setCursor('agt_1', 5)
    expect(repositories.messages.cursor('agt_1')).toBe(5)
    repositories.messages.setCursor('agt_1', 9)
    expect(repositories.messages.cursor('agt_1')).toBe(9)
    // An orchestrator message carries no sender and may target a task with an expiry.
    const systemMessage: NcagMessage = {
      ...message,
      id: newId('message'),
      sender: null,
      taskId: newId('task'),
      target: { kind: 'orchestrator' },
      expiresAtMs: 4242,
      deliveryStatus: 'EXPIRED',
    }
    repositories.messages.insert(systemMessage)
    expect(repositories.messages.get(systemMessage.id)).toEqual(systemMessage)
  })

  it('persists decisions, findings and evidence', () => {
    const opened = store()
    const repositories = createRepositories(opened.db)
    const created = project()
    repositories.projects.save(created)
    const workForDecision = task(created.id)
    const decision: DecisionRecord = {
      id: newId('decision'),
      projectId: created.id,
      taskId: workForDecision.id,
      category: 'material-architectural',
      author: 'orchestrator',
      problem: 'p',
      debate: {
        requested: 'r', belief: 'b', conflict: 'c', recommendation: 'rec',
        alternatives: [], evidence: ['e'], question: 'q',
      },
      recommendation: 'r',
      outcome: 'o',
      rationale: null,
      rejectedRecommendation: null,
      consequences: ['c'],
      affectedModules: ['m'],
      affectedTaskIds: [workForDecision.id],
      status: 'AWAITING_USER',
      createdAt: new Date().toISOString(),
      decidedAt: null,
      reviewHistory: [],
    }
    repositories.decisions.save(decision)
    expect(repositories.decisions.get(decision.id)).toEqual(decision)
    expect(repositories.decisions.get('dec_missing')).toBeNull()
    expect(repositories.decisions.listForProject(created.id)).toHaveLength(1)
    expect(repositories.decisions.pending()).toHaveLength(1)
    repositories.decisions.save({ ...decision, status: 'ACCEPTED', decidedAt: new Date().toISOString() })
    expect(repositories.decisions.pending()).toHaveLength(0)
    // A project-level decision carries no task, debate or affected tasks.
    const minimalDecision: DecisionRecord = {
      ...decision, id: newId('decision'), taskId: null, debate: null, affectedTaskIds: [], status: 'AWAITING_USER',
    }
    repositories.decisions.save(minimalDecision)
    const readMinimal = repositories.decisions.get(minimalDecision.id)
    expect(readMinimal?.taskId).toBeNull()
    expect(readMinimal?.debate).toBeNull()

    const work = task(created.id)
    repositories.tasks.save(work)
    const finding: ReviewFinding = {
      id: newId('finding'),
      taskId: work.id,
      projectId: created.id,
      reviewerAgentId: newId('agent'),
      reviewKind: 'code-review',
      severity: 'LOW',
      description: 'd',
      evidence: 'e',
      artifact: 'a',
      impact: 'i',
      recommendation: 'r',
      verificationMethod: 'v',
      status: 'OPEN',
      createdAt: new Date().toISOString(),
      resolvedAt: null,
    }
    repositories.findings.save(finding)
    expect(repositories.findings.listForTask(work.id)).toEqual([finding])
    expect(repositories.findings.open()).toHaveLength(1)
    repositories.findings.save({ ...finding, status: 'FIXED', resolvedAt: new Date().toISOString() })
    expect(repositories.findings.open()).toHaveLength(0)

    const criterion: AcceptanceCriterion = { id: 'ac-1', text: 't', verification: 'command', command: 'exit 0' }
    const evidence: VerificationEvidence = {
      taskId: work.id, criterionId: criterion.id, method: 'command', status: 'PASSED',
      command: 'exit 0', exitCode: 0, output: 'ok', recordedAt: new Date().toISOString(), producedBy: newId('agent'),
    }
    repositories.evidence.append(created.id, evidence)
    repositories.evidence.append(created.id, { ...evidence, criterionId: 'ac-2', producedBy: null })
    expect(repositories.evidence.listForTask(work.id)).toHaveLength(2)
    expect(repositories.evidence.listForTask(work.id)[1]?.producedBy).toBeNull()
  })

  it('persists leases, change sets, workspaces, tool calls, usage, checkpoints and counters', () => {
    const opened = store()
    const repositories = createRepositories(opened.db)
    const created = project()
    repositories.projects.save(created)
    const now = Date.now()
    const lease: ResourceLease = {
      id: newId('lease'),
      projectId: created.id,
      resourcePath: 'src/a.ts',
      mode: 'write',
      agentId: newId('agent'),
      taskId: newId('task'),
      fencingToken: 1,
      acquiredAtMs: now,
      renewedAtMs: now,
      expiresAtMs: now + 1000,
      status: 'ACTIVE',
      reason: 'edit',
    }
    repositories.leases.insert(lease)
    expect(repositories.leases.get(lease.id)).toEqual(lease)
    expect(repositories.leases.get('lse_missing')).toBeNull()
    expect(repositories.leases.active(created.id)).toHaveLength(1)
    expect(repositories.leases.maxFencingToken(created.id)).toBe(1)
    repositories.leases.update({ ...lease, status: 'RELEASED', expiresAtMs: now + 2000 })
    expect(repositories.leases.active(created.id)).toHaveLength(0)
    expect(repositories.leases.get(lease.id)?.status).toBe('RELEASED')
    const unattachedLease: ResourceLease = { ...lease, id: newId('lease'), status: 'ACTIVE', taskId: null }
    repositories.leases.insert(unattachedLease)
    expect(repositories.leases.get(unattachedLease.id)?.taskId).toBeNull()
    repositories.leases.releaseForAgent(lease.agentId, 'REVOKED')
    expect(repositories.leases.active(created.id)).toHaveLength(0)
    expect(repositories.leases.maxFencingToken('prj_none')).toBe(0)

    const work = task(created.id)
    repositories.tasks.save(work)
    const changeSet: ChangeSet = {
      id: newId('changeset'),
      projectId: created.id,
      taskId: work.id,
      agentId: lease.agentId,
      workspacePath: '/tmp/w',
      branch: 'ncag/a',
      baseCommit: 'abc',
      headCommit: 'def',
      changedFiles: ['src/a.ts'],
      message: 'm',
      status: 'SUBMITTED',
      submittedAt: new Date().toISOString(),
      report: ['r'],
      integratedCommit: null,
    }
    repositories.changeSets.save(changeSet)
    expect(repositories.changeSets.get(changeSet.id)).toEqual(changeSet)
    expect(repositories.changeSets.get('chg_missing')).toBeNull()
    expect(repositories.changeSets.listForProject(created.id)).toHaveLength(1)
    expect(repositories.changeSets.open(created.id)).toHaveLength(1)
    repositories.changeSets.save({ ...changeSet, status: 'VALIDATING' })
    expect(repositories.changeSets.open(created.id)).toHaveLength(1)
    repositories.changeSets.save({ ...changeSet, status: 'MERGED' })
    expect(repositories.changeSets.open(created.id)).toHaveLength(0)

    const workspace: WorkspaceRecord = {
      agentId: lease.agentId,
      projectId: created.id,
      taskId: work.id,
      path: '/tmp/w',
      branch: 'ncag/a',
      baseCommit: 'abc',
      createdAt: new Date().toISOString(),
      releasedAt: null,
    }
    repositories.workspaces.save(workspace)
    expect(repositories.workspaces.get(lease.agentId)).toEqual(workspace)
    expect(repositories.workspaces.get('agt_missing')).toBeNull()
    repositories.workspaces.save({ ...workspace, taskId: null })
    expect(repositories.workspaces.get(lease.agentId)?.taskId).toBeNull()
    repositories.workspaces.release(lease.agentId)
    expect(repositories.workspaces.get(lease.agentId)?.releasedAt).not.toBeNull()

    const toolCorrelation = newId('correlation')
    repositories.toolCalls.append({
      at: new Date().toISOString(), agentId: lease.agentId, projectId: created.id, taskId: work.id,
      tool: 'read', args: '{}', ok: true, error: null, output: 'o', durationMs: 3, correlationId: toolCorrelation,
    })
    expect(repositories.toolCalls.recent(lease.agentId, 5)).toHaveLength(1)
    expect(repositories.toolCalls.forCorrelation(created.id, toolCorrelation)).toHaveLength(1)
    expect(repositories.toolCalls.forCorrelation(created.id, newId('correlation'))).toEqual([])

    expect(repositories.usage.totals(created.id)).toEqual(EMPTY_USAGE)
    repositories.usage.append({
      at: new Date().toISOString(), projectId: created.id, taskId: work.id, agentId: lease.agentId,
      provider: 'p', model: 'm', inputTokens: 1, outputTokens: 2, cachedTokens: 0, costUsd: 0.5, purpose: 'agent-execution',
    })
    expect(repositories.usage.totals(created.id)).toEqual({ inputTokens: 1, outputTokens: 2, cachedTokens: 0, costUsd: 0.5, calls: 1 })
    expect(repositories.usage.totals('prj_none')).toEqual(EMPTY_USAGE)

    repositories.checkpoints.save({ id: newId('checkpoint'), projectId: created.id, at: new Date().toISOString(), reason: 'r', state: { a: 1 } })
    expect(repositories.checkpoints.listForProject(created.id)).toHaveLength(1)

    expect(repositories.toolInvocations.record(lease.agentId, 'fp')).toBe(1)
    expect(repositories.toolInvocations.record(lease.agentId, 'fp')).toBe(2)
    repositories.toolInvocations.reset(lease.agentId)
    expect(repositories.toolInvocations.record(lease.agentId, 'fp')).toBe(1)
  })

  it('sums usage across several ledger rows', () => {
    const opened = store()
    const repositories = createRepositories(opened.db)
    const created = project()
    repositories.projects.save(created)
    for (const costUsd of [0.25, 0.75]) {
      repositories.usage.append({
        at: new Date().toISOString(), projectId: created.id, taskId: null, agentId: null,
        provider: 'p', model: 'm', inputTokens: 10, outputTokens: 20, cachedTokens: 5, costUsd, purpose: 'agent-execution',
      })
    }
    expect(repositories.usage.totals(created.id)).toEqual({ inputTokens: 20, outputTokens: 40, cachedTokens: 10, costUsd: 1, calls: 2 })
  })
})
