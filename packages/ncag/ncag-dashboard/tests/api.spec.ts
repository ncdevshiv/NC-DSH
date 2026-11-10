import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { startDashboard } from '../src/index.ts'
import { runPlatform } from '../src/platform.ts'
import type { RunningPlatform } from '../src/platform.ts'
import { newId } from '@deepseek-ai/dsh-ncag-core'
import type { ChangeSet, Task, UserRequest } from '@deepseek-ai/dsh-ncag-core'

const platforms: RunningPlatform[] = []
const dashboards: { stop(): Promise<void> }[] = []
const directories: string[] = []

afterEach(async () => {
  while (dashboards.length > 0) await dashboards.pop()?.stop()
  while (platforms.length > 0) await platforms.pop()?.stop()
  while (directories.length > 0) {
    const directory = directories.pop()
    if (directory !== undefined) rmSync(directory, { recursive: true, force: true })
  }
})

async function start(token: string | null = 'token'): Promise<RunningPlatform> {
  const root = mkdtempSync(join(tmpdir(), 'ncag-api-'))
  directories.push(root)
  const platform = await runPlatform({
    NCAG_DATABASE_PATH: join(root, 'platform.db'),
    NCAG_WORKSPACE_ROOT: join(root, 'workspaces'),
    NCAG_MODEL_PRICING: JSON.stringify([{ provider: 'p', model: 'm', inputPerMillion: 1, outputPerMillion: 2 }]),
    NCAG_PORT: '0',
    NCAG_HOST: '127.0.0.1',
    ...(token === null ? {} : { NCAG_TOKEN: token }),
  }, null, null)
  platforms.push(platform)
  return platform
}

function headers(token: string | null = 'token'): Record<string, string> {
  return token === null ? {} : { authorization: `Bearer ${token}` }
}

async function call(platform: RunningPlatform, path: string, method = 'GET', body?: unknown): Promise<Response> {
  return fetch(platform.url.replace(/\/$/, '') + path, {
    method,
    headers: { 'content-type': 'application/json', ...headers() },
    ...(body === undefined ? {} : { body: typeof body === 'string' ? body : JSON.stringify(body) }),
  })
}

describe('ncag control-plane API routes', () => {
  it('serves every read and write operation over the orchestrator', async () => {
    const platform = await start()
    const orchestrator = platform.orchestrator
    const project = orchestrator.createProject({ name: 'api', description: 'd', repositoryPath: join(tmpdir(), 'ncag-api-repo') })

    const request: UserRequest = {
      id: newId('request'), projectId: project.id, rawText: 'build it', createdAt: new Date().toISOString(),
      intent: { goal: 'g', motivation: '', outcomes: [], constraints: [], technologies: [], scope: '', priority: 'P2', quality: '', security: [] },
      ambiguities: [], requirements: [], status: 'PLANNED',
    }
    orchestrator.repositories.requests.save(request)
    const now = new Date().toISOString()
    const work: Task = {
      id: newId('task'), projectId: project.id, requestId: request.id, requirementIds: [], title: 't', description: 'd',
      requiredCapability: 'general-implementation', assignedAgentId: null, dependencyIds: [], requiredInputs: [],
      expectedOutputs: [], acceptanceCriteria: [], requiredPermissions: [], contextRefs: [], priority: 'P2',
      deadlineMs: null, requiredReviews: [], requiredVerifications: [], status: 'PLANNED', depth: 0,
      createdAt: now, updatedAt: now, startedAt: null, completedAt: null,
      attempts: 0, reviewCycles: 0, consultation: false, statusReason: null,
    }
    orchestrator.repositories.tasks.save(work)
    const agent = orchestrator.agents.createInstance({
      profile: orchestrator.agents.select('general-implementation'),
      projectId: project.id, taskId: work.id, model: { provider: 'p', model: 'm' },
    })
    orchestrator.communication.send({
      projectId: project.id, sender: null, target: { kind: 'orchestrator' }, taskId: null, type: 'INFORMATION',
      subject: 's', content: 'c', correlationId: newId('correlation'),
    })
    const decision = orchestrator.decisions.propose({ projectId: project.id, taskId: null, category: 'user-reserved', author: 'orchestrator', problem: 'p', debate: null, recommendation: 'rec' })
    const holder = orchestrator.leases.acquire({ projectId: project.id, resourcePath: 'a.ts', mode: 'write', agentId: agent.id, taskId: work.id, reason: 'r' })
    expect(holder.granted).toBe(true)
    // A second writer is refused and queued, so the waiters route returns it.
    const blocked = orchestrator.leases.acquire({
      projectId: project.id, resourcePath: 'a.ts', mode: 'write', agentId: newId('agent'), taskId: work.id, reason: 'blocked',
    })
    expect(blocked.granted).toBe(false)
    const changeSet: ChangeSet = {
      id: newId('changeset'), projectId: project.id, taskId: work.id, agentId: agent.id, workspacePath: '/tmp/w',
      branch: 'ncag/a', baseCommit: 'a', headCommit: 'b', changedFiles: [], message: 'm', status: 'MERGED',
      submittedAt: now, report: [], integratedCommit: null,
    }
    orchestrator.repositories.changeSets.save(changeSet)

    const paths: readonly string[] = [
      '/api/snapshot', '/api/health', '/api/projects',
      `/api/projects/${project.id}/tasks`, `/api/projects/${project.id}/agents`, `/api/projects/${project.id}/messages`,
      `/api/projects/${project.id}/decisions`, `/api/projects/${project.id}/leases`, `/api/projects/${project.id}/changesets`,
      `/api/projects/${project.id}/events`, `/api/projects/${project.id}/waiters`,
      `/api/projects/${project.id}/trace?correlationId=${newId('correlation')}`,
      `/api/projects/${project.id}/profiles`, `/api/projects/${project.id}/requests`,
      `/api/projects/${project.id}/requirements`, `/api/projects/${project.id}/reviews`,
      `/api/tasks/${work.id}/changesets`, `/api/tasks/${work.id}/evidence`, `/api/tasks/${work.id}/findings`,
    ]
    for (const path of paths) expect((await call(platform, path)).status).toBe(200)
    const missingTask = await call(platform, `/api/tasks/${newId('task')}/changesets`)
    expect(missingTask.status).toBe(500)

    const created = await call(platform, '/api/projects', 'POST', { name: 'created', description: 'd', repositoryPath: join(tmpdir(), 'ncag-api-created'), baseBranch: 'main', instructions: 'x' })
    expect(created.status).toBe(201)

    const submitted = await call(platform, `/api/projects/${project.id}/requests`, 'POST', { text: 'do something' })
    expect(submitted.status).toBe(202)

    expect((await call(platform, `/api/projects/${project.id}/pause`, 'POST', { reason: 'testing' })).status).toBe(200)
    expect((await call(platform, `/api/projects/${project.id}/resume`, 'POST')).status).toBe(200)
    expect((await call(platform, `/api/tasks/${work.id}/cancel`, 'POST', {})).status).toBe(200)
    expect((await call(platform, `/api/decisions/${decision.id}/resolve`, 'POST', { outcome: 'rec', rationale: 'ok' })).status).toBe(200)
    expect((await call(platform, `/api/agents/${agent.id}/instruct`, 'POST', { text: 'carry on' })).status).toBe(200)
  })

  it('serves one correlation trace across audit, events and tool calls', async () => {
    const platform = await start()
    const orchestrator = platform.orchestrator
    const project = orchestrator.createProject({ name: 'trace', description: 'd', repositoryPath: join(tmpdir(), 'ncag-api-trace') })
    const correlation = newId('correlation')
    orchestrator.store.recordAudit({ category: 'task', action: 'traced', actor: 'o', projectId: project.id, correlationId: correlation })
    orchestrator.store.emitEvent({ kind: 'traced/event', projectId: project.id, summary: 's', data: { correlationId: correlation } })
    const response = await call(platform, `/api/projects/${project.id}/trace?correlationId=${encodeURIComponent(correlation)}`)
    expect(response.status).toBe(200)
    const body = await response.json() as { correlationId: string; audit: { action: string }[]; events: { kind: string }[] }
    expect(body.correlationId).toBe(correlation)
    expect(body.audit.map(entry => entry.action)).toEqual(['traced'])
    expect(body.events.map(event => event.kind)).toContain('traced/event')
    // The identifier is required: an absent one is an operator error, not an empty trace.
    expect((await call(platform, `/api/projects/${project.id}/trace`)).status).toBe(500)
  })

  it('rejects unauthenticated calls, unknown routes and malformed bodies', async () => {
    const platform = await start()
    const base = platform.url.replace(/\/$/, '')
    expect((await fetch(base + '/api/snapshot')).status).toBe(401)
    expect((await fetch(base + '/events')).status).toBe(401)
    expect((await fetch(base + '/events?token=token')).status).toBe(200)
    expect((await fetch(base + '/nope')).status).toBe(404)
    expect((await call(platform, '/api/nope')).status).toBe(404)
    expect((await call(platform, '/api/projects', 'POST', '{not json')).status).toBe(500)
    expect((await call(platform, '/api/projects', 'POST', { description: 'd' })).status).toBe(500)
    expect((await call(platform, '/api/projects', 'POST', { name: 3, description: 'd', repositoryPath: 'x' })).status).toBe(500)
    // A body above the size ceiling tears the request down before a response.
    await expect(fetch(base + '/api/projects', { method: 'POST', headers: { 'content-type': 'application/json', ...headers() }, body: 'x'.repeat(1_100_000) })).rejects.toThrow()
  })

  it('streams platform events and closes the stream on shutdown', async () => {
    const platform = await start()
    const controller = new AbortController()
    const stream = await fetch(platform.url.replace(/\/$/, '') + '/events', { headers: headers(), signal: controller.signal })
    expect(stream.status).toBe(200)
    const reader = stream.body?.getReader()
    if (reader === undefined) throw new Error('missing stream body')
    const decoder = new TextDecoder()
    expect(decoder.decode((await reader.read()).value)).toContain('connected')
    platform.orchestrator.createProject({ name: 'streamed', description: 'd', repositoryPath: join(tmpdir(), 'ncag-api-streamed') })
    const frame = decoder.decode((await reader.read()).value)
    expect(frame).toContain('data:')
    expect(frame).toContain('project/created')
    controller.abort()
  })

  it('rewrites a wildcard bind address to loopback and refuses a broken audit chain', async () => {
    const platform = await start(null)
    const wildcard = await startDashboard(platform.orchestrator, { port: 0, host: '0.0.0.0', token: null })
    dashboards.push(wildcard)
    expect(wildcard.url.startsWith('http://127.0.0.1:')).toBe(true)
    const ipv6 = await startDashboard(platform.orchestrator, { port: 0, host: '::', token: null })
    dashboards.push(ipv6)
    expect(ipv6.url.startsWith('http://127.0.0.1:')).toBe(true)
    // Tamper the audit chain and observe the health endpoint report it.
    platform.orchestrator.store.recordAudit({ category: 'task', action: 'x', actor: 'o', correlationId: newId('correlation') })
    platform.orchestrator.store.db.prepare('UPDATE audit SET action = ? WHERE sequence = 1').run('tampered')
    const health = await fetch(platform.url.replace(/\/$/, '') + '/api/health')
    expect(health.status).toBe(500)
    const body = await health.json() as { status: string }
    expect(body.status).toBe('audit-chain-broken')
  })
})
