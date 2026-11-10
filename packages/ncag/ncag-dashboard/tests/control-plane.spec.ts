import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { runPlatform } from '../src/platform.ts'
import type { RunningPlatform } from '../src/platform.ts'

const platforms: RunningPlatform[] = []

afterEach(async () => {
  while (platforms.length > 0) {
    const platform = platforms.pop()
    if (platform !== undefined) await platform.stop()
  }
})

function start(): Promise<RunningPlatform> {
  const root = mkdtempSync(join(tmpdir(), 'ncag-dashboard-'))
  const promise = runPlatform({
    NCAG_DATABASE_PATH: join(root, 'platform.db'),
    NCAG_WORKSPACE_ROOT: join(root, 'workspaces'),
    NCAG_MODEL_PRICING: JSON.stringify([{ provider: 'p', model: 'm', inputPerMillion: 1, outputPerMillion: 2 }]),
    NCAG_PORT: '0',
    NCAG_TOKEN: 'token',
    NCAG_HOST: '127.0.0.1',
  }, null, null)
  return promise.then(async (platform) => {
    platforms.push({ ...platform, stop: async () => { await platform.stop(); rmSync(root, { recursive: true, force: true }) } })
    return platforms[platforms.length - 1] as RunningPlatform
  })
}

async function call(platform: RunningPlatform, path: string, method = 'GET', body?: unknown): Promise<Response> {
  return fetch(platform.url.replace(/\/$/, '') + path, {
    method,
    headers: { 'content-type': 'application/json', authorization: 'Bearer token' },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  })
}

describe('ncag control plane', () => {
  it('serves the dashboard, rejects unauthenticated calls, and performs real operations', async () => {
    const platform = await start()
    const html = await fetch(platform.url)
    expect(html.status).toBe(200)
    const document = await html.text()
    expect(document).toContain('NC-AG Control Plane')
    expect(document).toContain('/api/snapshot')
    // Every control in the document calls a route the server actually serves.
    expect(document).toContain("api('/api/projects/' + projectId + '/messages')")
    expect(document).toContain("api('/api/tasks/' + taskId + '/evidence')")
    expect(document).toContain("api('/api/tasks/' + taskId + '/findings')")
    expect(document).toContain("api('/api/tasks/' + taskId + '/changesets')")
    expect(document).toContain("api('/api/projects/' + projectId + '/waiters')")
    expect(document).toContain("api('/api/projects/' + projectId + '/requirements')")
    expect(document).toContain("api('/api/projects/' + projectId + '/reviews')")
    expect(document).toContain("api('/api/agents/' + agent.id + '/cancel'")
    expect(document).toContain("api('/api/tasks/' + agent.taskId + '/reassign'")
    const anonymous = await fetch(platform.url + 'api/snapshot')
    expect(anonymous.status).toBe(401)
    const health = await call(platform, '/api/health')
    expect(health.status).toBe(200)
    const project = await call(platform, '/api/projects', 'POST', {
      name: 'control plane',
      description: 'exercise the API',
      repositoryPath: join(tmpdir(), 'ncag-dashboard-repository'),
    })
    expect(project.status).toBe(201)
    const snapshot = await call(platform, '/api/snapshot')
    const body = await snapshot.json() as { projects: { project: { name: string } }[] }
    expect(body.projects.length).toBe(1)
    expect(body.projects[0]?.project.name).toBe('control plane')
    const events = platform.orchestrator.store.recentEvents(20)
    expect(events.some(event => event.kind === 'project/created')).toBe(true)
  })
  it('reports the health of the audit chain', async () => {
    const platform = await start()
    const health = await call(platform, '/api/health')
    const body = await health.json() as { status: string; audit: { brokenAt: number | null } }
    expect(body.status).toBe('ok')
    expect(body.audit.brokenAt).toBeNull()
  })
})
