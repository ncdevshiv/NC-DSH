/**
 * Model-facing NC-AG tools. They let an agent inside the harness create a
 * project, submit a request, inspect tasks and agents, answer a pending
 * decision, read the platform health and read one project's timeline. Each
 * tool performs the real operation through the orchestrator.
 * @module @deepseek-ai/dsh-tool-ncag/tools
 */

import { defineTool } from '@deepseek-ai/dsh-tools'
import type { ToolDefinition } from '@deepseek-ai/dsh-tools'
import type { Context } from '@deepseek-ai/cordis'
// Type-only: brings the `ctx.ncag` service declaration into this program.
import type {} from '@deepseek-ai/dsh-ncag-service'

/**
 * Build every NC-AG tool over the mounted service.
 * @param ctx - the owning context, which must provide the NC-AG service.
 * @returns the tool definitions, ready for the registry.
 */
export function createNcagTools(ctx: Context): ToolDefinition[] {
  const platform = () => {
    const service = ctx.get('ncag')
    if (service === undefined) throw new Error('ncag: the NC-AG service is not mounted')
    return service
  }

  const projectTool = defineTool({
    name: 'ncag_project',
    description: 'Create an NC-AG project: the durable container of one autonomous multi-agent workstream. Creating a project does not start work; ncag_request does.',
    parameters: {
      name: { type: 'string', required: true, description: 'Project name.' },
      description: { type: 'string', required: true, description: 'What the project must deliver.' },
      repositoryPath: { type: 'string', description: 'Absolute repository path; defaults to the current workspace.' },
      instructions: { type: 'string', description: 'Project-wide instructions every agent receives.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          id: { type: 'string' },
          name: { type: 'string' },
          status: { type: 'string' },
          repositoryPath: { type: 'string' },
        },
      },
      render: (_args, value) => [{ type: 'text', text: `project ${value.name ?? ''} (${value.id ?? ''}) is ${value.status ?? ''} at ${value.repositoryPath ?? ''}` }],
    },
    execute: (args) => {
      const service = platform()
      const project = args.repositoryPath === undefined
        ? service.createProjectFromWorkspace(args.name, args.description)
        : service.orchestrator.createProject({
          name: args.name,
          description: args.description,
          repositoryPath: args.repositoryPath,
          ...(args.instructions === undefined ? {} : { instructions: args.instructions }),
        })
      return Promise.resolve({ id: project.id, name: project.name, status: project.status, repositoryPath: project.repositoryPath })
    },
  })

  const requestTool = defineTool({
    name: 'ncag_request',
    description: 'Submit a work request to an NC-AG project. The platform analyzes it, normalizes requirements, builds the task graph, spawns the agents it needs and executes the work. Returns the planned tasks.',
    parameters: {
      projectId: { type: 'string', required: true, description: 'Project identifier returned by ncag_project.' },
      text: { type: 'string', required: true, description: 'The complete request.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          projectId: { type: 'string' },
          tasks: {
            type: 'array',
            items: {
              type: 'object',
              additionalProperties: false,
              properties: {
                id: { type: 'string' },
                title: { type: 'string' },
                status: { type: 'string' },
                capability: { type: 'string' },
              },
            },
          },
        },
      },
      render: (_args, value) => [{ type: 'text', text: (value.tasks ?? [])
        .map(task => `- ${task.id ?? ''} [${task.status ?? ''}] ${task.title ?? ''} (${task.capability ?? ''})`)
        .join('\n') }],
    },
    execute: async (args) => {
      const service = platform()
      const result = await service.orchestrator.submitRequest({ projectId: args.projectId, rawText: args.text })
      return {
        projectId: result.request.projectId,
        tasks: result.tasks.map(task => ({ id: task.id, title: task.title, status: task.status, capability: task.requiredCapability })),
      }
    },
  })

  const statusTool = defineTool({
    name: 'ncag_status',
    description: 'Read the NC-AG platform snapshot: projects, tasks, agents, pending decisions, leases, usage and the audit chain state.',
    parameters: {
      projectId: { type: 'string', description: 'Restrict the report to one project.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: { report: { type: 'string' } },
      },
      render: (_args, value) => [{ type: 'text', text: value.report ?? '' }],
    },
    execute: async (args) => {
      const service = platform()
      const snapshot = await service.orchestrator.snapshot()
      const entries = args.projectId === undefined
        ? snapshot.projects
        : snapshot.projects.filter(entry => entry.project.id === args.projectId)
      const report: string[] = []
      report.push('audit chain: ' + (snapshot.audit.brokenAt === null ? 'verified' : 'broken at ' + String(snapshot.audit.brokenAt)) + ' (' + String(snapshot.audit.records) + ' records)')
      report.push('cost: ' + snapshot.usage.costUsd.toFixed(4) + ' USD across ' + String(snapshot.usage.calls) + ' model call(s); routes ' + String(snapshot.routes.length))
      for (const entry of entries) {
        report.push(entry.project.name + ' [' + entry.project.status + '] tasks ' + String(entry.tasks.length) + ', agents ' + String(entry.agents.length) + ', leases ' + String(entry.leases) + ', pending decisions ' + String(entry.decisionsPending))
        for (const task of entry.tasks) report.push('  task ' + task.id + ' [' + task.status + '] ' + task.title)
        for (const agent of entry.agents) report.push('  agent ' + agent.id + ' [' + agent.status + '] ' + agent.profileKey + ' steps ' + String(agent.steps))
      }
      return { report: report.join('\n') }
    },
  })

  const decideTool = defineTool({
    name: 'ncag_decide',
    description: 'Answer a pending NC-AG decision. The platform records the chosen approach permanently; when it differs from the recommendation, the rejection and its consequences are recorded too.',
    parameters: {
      decisionId: { type: 'string', required: true, description: 'Decision identifier.' },
      outcome: { type: 'string', required: true, description: 'The approach that was chosen.' },
      rationale: { type: 'string', description: 'The rationale for the choice.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          decisionId: { type: 'string' },
          status: { type: 'string' },
          outcome: { type: 'string' },
        },
      },
      render: (_args, value) => [{ type: 'text', text: `decision ${value.decisionId ?? ''} is ${value.status ?? ''}: ${value.outcome ?? ''}` }],
    },
    execute: (args) => {
      const service = platform()
      const decision = service.orchestrator.resolveDecision(args.decisionId, args.outcome, args.rationale ?? null)
      return Promise.resolve({ decisionId: decision.id, status: decision.status, outcome: decision.outcome })
    },
  })

  const eventsTool = defineTool({
    name: 'ncag_events',
    description: 'Read the NC-AG timeline for one project: the ordered platform events that record planning, agent spawn, tool calls, reviews, verification, integration, decisions and recovery.',
    parameters: {
      projectId: { type: 'string', required: true, description: 'Project identifier returned by ncag_project.' },
      limit: { type: 'string', description: 'Maximum events to read, newest last. Defaults to 50 and is capped at 500.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          events: {
            type: 'array',
            items: {
              type: 'object',
              additionalProperties: false,
              properties: {
                at: { type: 'string' },
                kind: { type: 'string' },
                summary: { type: 'string' },
              },
            },
          },
        },
      },
      render: (_args, value) => [{ type: 'text', text: (value.events ?? [])
        .map(event => `${event.at ?? ''} [${event.kind ?? ''}] ${event.summary ?? ''}`)
        .join('\n') }],
    },
    execute: (args) => {
      const service = platform()
      // The project must exist; reading a timeline for an unknown project
      // would report an empty history that looks like a quiet project.
      service.orchestrator.project(args.projectId)
      const requested = Number(args.limit ?? '50')
      const limit = Number.isFinite(requested) ? Math.min(500, Math.max(1, Math.trunc(requested))) : 50
      const events = service.orchestrator.store.recentEvents(limit, args.projectId)
      return Promise.resolve({
        events: events.map(event => ({ at: event.at, kind: event.kind, summary: event.summary })),
      })
    },
  })

  return [projectTool, requestTool, statusTool, decideTool, eventsTool]
}
