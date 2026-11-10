/**
 * Model-facing workspace retrieval tools over `ctx.retrieval`.
 *
 * @module @deepseek-ai/dsh-tool-retrieval
 */

import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import { HarnessError } from '@deepseek-ai/dsh-llm'
import type { RetrievalDocument, RetrievalSearchMode, RetrievalSearchFilter } from '@deepseek-ai/dsh-retrieval'
import { defineTool } from '@deepseek-ai/dsh-tools'
import type { ToolRunContext } from '@deepseek-ai/dsh-tools'
import type {} from '@deepseek-ai/dsh-working-directory'
import type { Agent } from '@deepseek-ai/dsh-agent'
import type {} from '@deepseek-ai/dsh-system-prompt'

/** Cordis plugin name used by Loader diagnostics. */
export const name = 'tool-retrieval'

/** Services required by the model-facing retrieval tools. */
export const inject = ['tools', 'systemPrompt', 'retrieval', 'workingDirectory']

/** Default maximum hits returned by one search. */
export const DEFAULT_MAX_RESULTS = 20

/** Default cooperative deadline for one search. */
export const DEFAULT_TIMEOUT_MS = 30_000

/** Deployment-owned retrieval tool bounds. */
export interface Config {
  /** Maximum hits returned by one search. Defaults to 20. */
  maxResults?: number
  /** Cooperative search deadline in milliseconds. Defaults to 30000. */
  timeoutMs?: number
}

/** Schemastery config for Loader defaults and generated configuration docs. */
export const Config: z<Config> = z.object({
  maxResults: z.number().step(1).min(1).max(1000).default(DEFAULT_MAX_RESULTS),
  timeoutMs: z.number().step(1).min(1).default(DEFAULT_TIMEOUT_MS),
})

interface ToolSearchArgs {
  query: string
  mode?: RetrievalSearchMode
  path?: string[]
  session_id?: string[]
  event_types?: string[]
  event_surfaces?: string[]
  time_from?: string
  time_to?: string
}

interface SearchFilterArgs {
  path?: string[]
  session_id?: string[]
  event_types?: string[]
  event_surfaces?: string[]
  time_from?: string
  time_to?: string
}

interface IndexArgs {
  documents: Array<{
    id: string
    text: string
    path?: string
    session_id?: string
    seq?: number
    type?: string
    surface?: string
    time?: string
    vector?: number[]
  }>
}

const TEXT_OUTPUT = {
  schema: { type: 'string' as const },
  render: (_args: unknown, value: string) => [{ type: 'text' as const, text: value }],
}

const SEARCH_PROMPT =
  'Use workspace_search to find relevant project files and prior session events. Lexical mode matches text exactly; '
  + 'semantic and hybrid modes rank by meaning and require an embedding-capable deployment. '
  + 'Results are workspace-scoped and each hit names its source.'

const INDEX_PROMPT =
  'Use workspace_index to add or refresh searchable workspace documents. Pass stable ids; re-indexing the same id replaces it.'

const SEARCH_PARAMETERS = {
  query: { type: 'string', required: true, description: 'Search text interpreted as data, never index syntax.' },
  mode: {
    type: 'string',
    enum: ['lexical', 'semantic', 'hybrid'],
    description: 'Ranking mode. Defaults to lexical, which matches text exactly.',
  },
  path: { type: 'array', items: { type: 'string' }, description: 'Workspace-relative paths to include.' },
  session_id: { type: 'array', items: { type: 'string' }, description: 'Session identities to include.' },
  event_types: { type: 'array', items: { type: 'string' }, description: 'Session event types to include.' },
  event_surfaces: {
    type: 'array',
    items: { type: 'string', enum: ['current', 'shadowed', 'log-only'] },
    description: 'Session event surfaces to include.',
  },
  time_from: { type: 'string', description: 'Inclusive timezone-qualified ISO 8601 lower bound.' },
  time_to: { type: 'string', description: 'Inclusive timezone-qualified ISO 8601 upper bound.' },
} as const

const INDEX_PARAMETERS = {
  documents: {
    type: 'array',
    required: true,
    items: {
      type: 'object',
      additionalProperties: false,
      properties: {
        id: { type: 'string', required: true, description: 'Stable document identity.' },
        text: { type: 'string', required: true, description: 'Searchable text.' },
        path: { type: 'string', description: 'Workspace-relative file path.' },
        session_id: { type: 'string', description: 'Owning session identity.' },
        seq: { type: 'integer', description: 'Session event sequence.' },
        type: { type: 'string', description: 'Session event type.' },
        surface: { type: 'string', enum: ['current', 'shadowed', 'log-only'], description: 'Session event surface.' },
        time: { type: 'string', description: 'Timezone-qualified ISO 8601 event time.' },
        vector: { type: 'array', items: { type: 'number' }, description: 'Caller-supplied dense vector.' },
      },
    },
    description: 'Documents to add or replace.',
  },
} as const

/**
 * Register the workspace retrieval tools and shared model guidance.
 * @param ctx - Plugin context; registrations are effects scoped to this plugin.
 * @param config - resolved plugin configuration.
 */
export function apply(ctx: Context, config: Config): void {
  /* v8 ignore start -- Loader's Config schema fills these defaults before apply */
  const maxResults = config.maxResults ?? DEFAULT_MAX_RESULTS
  const timeoutMs = config.timeoutMs ?? DEFAULT_TIMEOUT_MS
  /* v8 ignore stop */
  ctx.systemPrompt.section({
    name: 'tool:retrieval',
    order: ctx.systemPrompt.getSectionOrder('TOOL_RETRIEVAL'),
    text: `${SEARCH_PROMPT}\n${INDEX_PROMPT}`,
  })

  ctx.tools.register(defineTool({
    name: 'workspace_search',
    description: 'Search workspace files and prior session events by text or meaning.',
    parameters: SEARCH_PARAMETERS,
    output: TEXT_OUTPUT,
    timeoutMs,
    execute: async (args, exec) => executeSearch(ctx, args, exec, maxResults),
    presentCall: args => ({
      card: 'generic',
      kind: 'search',
      title: 'Search workspace',
      rawInput: args.query,
    }),
  }))

  ctx.tools.register(defineTool({
    name: 'workspace_index',
    description: 'Add or replace searchable workspace documents by stable id.',
    parameters: INDEX_PARAMETERS,
    output: TEXT_OUTPUT,
    timeoutMs,
    execute: async (args, exec) => executeIndex(ctx, args, exec),
  }))
}

async function executeSearch(
  ctx: Context,
  args: ToolSearchArgs,
  exec: ToolRunContext,
  maxResults: number,
): Promise<string> {
  const workspaceRoot = activeWorkspace(ctx, exec)
  const filter = buildFilter(args)
  const page = await callRetrieval(ctx, exec, 'workspace search', () => ctx.retrieval.search({
    workspaceRoot,
    query: args.query,
    mode: args.mode ?? 'lexical',
    limit: maxResults,
    ...filter === undefined ? {} : { filter },
  }, exec.signal))
  if (page.items.length === 0) return 'No workspace matches found.'
  const lines = [`Workspace matches (${String(page.items.length)}):`]
  for (const [index, hit] of page.items.entries()) {
    /* v8 ignore next -- the provider always stores a path for file hits and a seq for session hits */
    const location = hit.path ?? (hit.sessionId === undefined ? hit.id : `session ${hit.sessionId} seq ${String(hit.seq ?? 0)}`)
    lines.push(
      '',
      `${String(index + 1)}. ${location}`,
      `   Scope: ${hit.scope} | Score: ${hit.score.toFixed(3)}`,
      `   ${snippet(hit.text)}`,
    )
  }
  if (page.nextCursor !== undefined) {
    lines.push('', 'More matches exist. Narrow the query or filters to see fewer, more relevant results.')
  }
  return lines.join('\n')
}

async function executeIndex(ctx: Context, args: IndexArgs, exec: ToolRunContext): Promise<string> {
  const workspaceRoot = activeWorkspace(ctx, exec)
  if (!Array.isArray(args.documents) || args.documents.length === 0) {
    throw new HarnessError('workspace_index requires at least one document', 'RETRIEVAL_TOOL_INVALID_ARGUMENTS')
  }
  const documents: RetrievalDocument[] = args.documents.map((document) => {
    const time = document.time === undefined ? undefined : parseTimestamp(document.time)
    return {
      id: document.id,
      scope: document.session_id === undefined ? 'workspace' : 'session',
      text: document.text,
      ...document.path === undefined ? {} : { path: document.path },
      ...document.session_id === undefined ? {} : { sessionId: document.session_id },
      ...document.seq === undefined ? {} : { seq: document.seq },
      ...document.type === undefined ? {} : { type: document.type },
      ...document.surface === undefined ? {} : { surface: document.surface },
      ...time === undefined ? {} : { time },
      ...document.vector === undefined ? {} : { vector: document.vector },
    }
  })
  await callRetrieval(ctx, exec, 'workspace index', () => ctx.retrieval.indexDocuments({ workspaceRoot, documents }, exec.signal))
  return `Indexed ${String(documents.length)} workspace document${documents.length === 1 ? '' : 's'}.`
}

function activeWorkspace(ctx: Context, exec: ToolRunContext): string {
  const agent: Agent | undefined = exec.agent
  if (agent === undefined) {
    throw new HarnessError('workspace retrieval requires an agent-bound caller', 'RETRIEVAL_TOOL_MISSING_AGENT')
  }
  // The original project header keeps retrieval data inside the repository
  // even after a Session changes its effective working directory.
  return agent.session.header.cwd ?? ctx.workingDirectory.get(agent.session)
}

function buildFilter(args: SearchFilterArgs): RetrievalSearchFilter | undefined {
  const timeFrom = args.time_from === undefined ? undefined : parseTimestamp(args.time_from)
  const timeTo = args.time_to === undefined ? undefined : parseTimestamp(args.time_to)
  if (timeFrom !== undefined && timeTo !== undefined && timeFrom > timeTo) {
    throw new HarnessError('time_from must not exceed time_to', 'RETRIEVAL_TOOL_INVALID_ARGUMENTS')
  }
  if (
    args.path === undefined
    && args.session_id === undefined
    && args.event_types === undefined
    && args.event_surfaces === undefined
    && timeFrom === undefined
    && timeTo === undefined
  ) return undefined
  return {
    ...args.path === undefined ? {} : { path: args.path },
    ...args.session_id === undefined ? {} : { sessionId: args.session_id },
    ...args.event_types === undefined ? {} : { type: args.event_types },
    ...args.event_surfaces === undefined ? {} : { surface: args.event_surfaces },
    ...timeFrom === undefined ? {} : { timeFrom },
    ...timeTo === undefined ? {} : { timeTo },
  }
}

function parseTimestamp(value: string): number {
  const parsed = Date.parse(value)
  if (!Number.isSafeInteger(parsed)) {
    throw new HarnessError('time bounds must be valid ISO 8601 timestamps', 'RETRIEVAL_TOOL_INVALID_ARGUMENTS')
  }
  return parsed
}

async function callRetrieval<Value>(
  ctx: Context,
  exec: ToolRunContext,
  operation: string,
  invoke: () => Promise<Value>,
): Promise<Value> {
  exec.signal.throwIfAborted()
  try {
    const value = await invoke()
    exec.signal.throwIfAborted()
    return value
  } catch (error: unknown) {
    exec.signal.throwIfAborted()
    /* v8 ignore next -- provider failures are Error instances */
    ctx.logger.warn(`tool-retrieval: ${operation} failed: ${error instanceof Error ? error.message : String(error)}`)
    throw new HarnessError(
      operation === 'workspace search'
        ? 'workspace search is unavailable'
        : 'workspace indexing is unavailable',
      'RETRIEVAL_TOOL_FAILED',
    )
  }
}

function snippet(text: string, limit = 240): string {
  const flat = text.replace(/\s+/g, ' ').trim()
  const characters = Array.from(flat)
  return characters.length <= limit ? flat : `${characters.slice(0, limit - 1).join('')}…`
}
