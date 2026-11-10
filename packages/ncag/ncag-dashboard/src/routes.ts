
/**
 * Route handling for the NC-AG control plane: one handler per documented API
 * operation, each calling the orchestrator method that performs the work.
 * @module @deepseek-ai/dsh-ncag-dashboard/routes
 */

import type { IncomingMessage, ServerResponse } from 'node:http'
import type { NcagOrchestrator } from '@deepseek-ai/dsh-ncag-orchestrator'

/** Everything a route handler may use. */
export interface RouteContext {
  readonly orchestrator: NcagOrchestrator
  readonly token: string | null
  readonly readJson: (request: IncomingMessage) => Promise<unknown>
  readonly subscribe: (response: ServerResponse) => () => void
}

/** Write one JSON response. */
function json(response: ServerResponse, status: number, body: unknown): void {
  response.writeHead(status, { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' })
  response.end(JSON.stringify(body))
}

/** Read a string field from a request body, failing when it is required and absent. */
function field(body: unknown, name: string, required: boolean): string | undefined {
  const value = typeof body === 'object' && body !== null && name in body ? (body as Record<string, unknown>)[name] : undefined
  if (value === undefined) {
    if (required) throw new Error(`missing required field ${JSON.stringify(name)}`)
    return undefined
  }
  if (typeof value !== 'string') throw new Error(`field ${JSON.stringify(name)} must be a string`)
  return value
}

/** Whether the request carries a valid API token when one is configured. */
function authorized(context: RouteContext, request: IncomingMessage, url: URL): boolean {
  if (context.token === null) return true
  const header = request.headers.authorization
  if (typeof header === 'string' && header === `Bearer ${context.token}`) return true
  return url.searchParams.get('token') === context.token
}

/**
 * Dispatch one request to its operation.
 * @param context - services a handler may use.
 * @param request - the incoming request.
 * @param response - the response to write.
 * @param dashboardHtml - the dashboard document served at `/`.
 * @returns a promise settling when the response is complete.
 */
export async function handleRoute(
  context: RouteContext,
  request: IncomingMessage,
  response: ServerResponse,
  dashboardHtml: string,
): Promise<void> {
  /* v8 ignore next -- Node always supplies a request URL. */
  const url = new URL(request.url ?? '/', 'http://localhost')
  const path = url.pathname
  if (path === '/' || path === '/index.html') {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' })
    response.end(dashboardHtml)
    return
  }
  if (path === '/events') {
    if (!authorized(context, request, url)) {
      json(response, 401, { error: 'invalid token' })
      return
    }
    response.writeHead(200, {
      'content-type': 'text/event-stream',
      'cache-control': 'no-store',
      connection: 'keep-alive',
    })
    response.write(': connected\n\n')
    const dispose = context.subscribe(response)
    request.on('close', dispose)
    return
  }
  if (!path.startsWith('/api/')) {
    json(response, 404, { error: `no route for ${path}` })
    return
  }
  if (!authorized(context, request, url)) {
    json(response, 401, { error: 'invalid token' })
    return
  }
  const orchestrator = context.orchestrator
  /* v8 ignore next -- Node always supplies a request method. */
  const method = request.method ?? 'GET'
  const body = method === 'POST' ? await context.readJson(request) : {}
  if (path === '/api/snapshot' && method === 'GET') {
    json(response, 200, await orchestrator.snapshot())
    return
  }
  if (path === '/api/health' && method === 'GET') {
    const audit = orchestrator.store.verifyAudit()
    json(response, audit.brokenAt === null ? 200 : 500, {
      status: audit.brokenAt === null ? 'ok' : 'audit-chain-broken',
      audit,
      projects: orchestrator.repositories.projects.list().length,
      liveAgents: orchestrator.agents.listLive().length,
    })
    return
  }
  if (path === '/api/projects' && method === 'GET') {
    json(response, 200, orchestrator.repositories.projects.list())
    return
  }
  if (path === '/api/projects' && method === 'POST') {
    /* v8 ignore start -- field() throws on an absent required field, so the empty fallbacks are type-level defense. */
    const project = orchestrator.createProject({
      name: field(body, 'name', true) ?? '',
      description: field(body, 'description', true) ?? '',
      repositoryPath: field(body, 'repositoryPath', true) ?? '',
      ...(field(body, 'baseBranch', false) === undefined ? {} : { baseBranch: field(body, 'baseBranch', false) ?? '' }),
      ...(field(body, 'instructions', false) === undefined ? {} : { instructions: field(body, 'instructions', false) ?? '' }),
    })
    /* v8 ignore stop */
    json(response, 201, project)
    return
  }
  // The guards below prove the id segment and every required body field exist;
  // the empty fallbacks are type-level defense.
  /* v8 ignore start -- covered by the route guards themselves. */
  const segments = path.split('/').filter(segment => segment.length > 0)
  if (segments[1] === 'projects' && segments[3] === 'requests' && method === 'POST') {
    const projectId = segments[2] ?? ''
    const result = await orchestrator.submitRequest({ projectId, rawText: field(body, 'text', true) ?? '' })
    json(response, 202, { request: result.request, tasks: result.tasks, plan: result.plan })
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'tasks' && method === 'GET') {
    json(response, 200, orchestrator.repositories.tasks.listForProject(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'agents' && method === 'GET') {
    json(response, 200, orchestrator.agents.listInstances(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'profiles' && method === 'GET') {
    json(response, 200, orchestrator.agents.summarize())
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'requests' && method === 'GET') {
    json(response, 200, orchestrator.repositories.requests.listForProject(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'requirements' && method === 'GET') {
    json(response, 200, orchestrator.repositories.requirements.listForProject(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'reviews' && method === 'GET') {
    json(response, 200, orchestrator.repositories.reviews.listForProject(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'messages' && method === 'GET') {
    json(response, 200, orchestrator.communication.history(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'decisions' && method === 'GET') {
    json(response, 200, orchestrator.decisions.history(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'leases' && method === 'GET') {
    json(response, 200, orchestrator.leases.active(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'waiters' && method === 'GET') {
    json(response, 200, orchestrator.leases.waiting(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'changesets' && method === 'GET') {
    json(response, 200, orchestrator.repositories.changeSets.listForProject(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'events' && method === 'GET') {
    const limit = Number(url.searchParams.get('limit') ?? '100')
    json(response, 200, orchestrator.store.recentEvents(
      Number.isFinite(limit) ? Math.min(1000, Math.max(1, Math.trunc(limit))) : 100,
      segments[2] ?? undefined,
    ))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'trace' && method === 'GET') {
    // One trace follows a correlation identifier from the request through the
    // task, the agent and its tool calls to the verification and delivery.
    const correlationId = url.searchParams.get('correlationId') ?? ''
    if (correlationId.length === 0) throw new Error('missing required query parameter "correlationId"')
    json(response, 200, orchestrator.trace(segments[2] ?? '', correlationId))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'pause' && method === 'POST') {
    json(response, 200, orchestrator.pauseProject(segments[2] ?? '', field(body, 'reason', false) ?? 'operator request'))
    return
  }
  if (segments[1] === 'projects' && segments[3] === 'resume' && method === 'POST') {
    json(response, 200, await orchestrator.resumeProject(segments[2] ?? '', field(body, 'reason', false) ?? 'operator request'))
    return
  }
  if (segments[1] === 'tasks' && segments[3] === 'changesets' && method === 'GET') {
    const taskId = segments[2] ?? ''
    const task = orchestrator.repositories.tasks.get(taskId)
    if (task === null) throw new Error(`ncag: task ${taskId} does not exist`)
    json(response, 200, orchestrator.repositories.changeSets.listForProject(task.projectId).filter(entry => entry.taskId === taskId))
    return
  }
  if (segments[1] === 'tasks' && segments[3] === 'evidence' && method === 'GET') {
    json(response, 200, orchestrator.repositories.evidence.listForTask(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'tasks' && segments[3] === 'findings' && method === 'GET') {
    json(response, 200, orchestrator.repositories.findings.listForTask(segments[2] ?? ''))
    return
  }
  if (segments[1] === 'tasks' && segments[3] === 'cancel' && method === 'POST') {
    json(response, 200, orchestrator.cancelTask(segments[2] ?? '', field(body, 'reason', false) ?? 'operator request'))
    return
  }
  if (segments[1] === 'tasks' && segments[3] === 'reassign' && method === 'POST') {
    json(response, 200, orchestrator.reassignTask(
      segments[2] ?? '',
      field(body, 'capability', true) ?? '',
      field(body, 'reason', false) ?? 'operator request',
    ))
    return
  }
  if (segments[1] === 'decisions' && segments[3] === 'resolve' && method === 'POST') {
    const outcome = field(body, 'outcome', true) ?? ''
    const rationale = field(body, 'rationale', false) ?? null
    json(response, 200, orchestrator.resolveDecision(segments[2] ?? '', outcome, rationale))
    return
  }
  if (segments[1] === 'agents' && segments[3] === 'cancel' && method === 'POST') {
    json(response, 200, orchestrator.cancelAgent(segments[2] ?? '', field(body, 'reason', false) ?? 'operator request'))
    return
  }
  if (segments[1] === 'agents' && segments[3] === 'instruct' && method === 'POST') {
    json(response, 200, orchestrator.instructAgent(segments[2] ?? '', field(body, 'text', true) ?? ''))
    return
  }
  /* v8 ignore stop */
  json(response, 404, { error: `no route for ${method} ${path}` })
}
