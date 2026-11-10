/**
 * Sandboxed file and command tools the NC-AG agents execute, plus the two
 * coordination tools through which an agent asks for help or hands a task
 * off. Every path is resolved inside the agent workspace and refused when it
 * escapes, so a workspace-confined agent cannot read or write outside its own
 * worktree; coordination requests reach the orchestrator through
 * `ToolEnvironment.coordination` rather than touching another agent directly.
 * @module @deepseek-ai/dsh-ncag-runtime/tools
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync, statSync, readdirSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path'
import type { AgentToolSchema, HandoffRequest, HelpRequest, ToolEnvironment, ToolOutcome } from './ports.ts'

/** Maximum bytes a single read tool call returns. */
export const MAX_READ_BYTES = 200_000

/** Maximum bytes a shell tool call returns. */
export const MAX_COMMAND_OUTPUT_BYTES = 60_000

/** Default shell timeout. */
export const COMMAND_TIMEOUT_MS = 180_000

/** Default shell executable when neither the environment nor a platform fallback resolves one. */
export const DEFAULT_SHELL = 'pwsh'

/**
 * Resolve the shell executable used to run commands. A deployment overrides it
 * through NCAG_SHELL; otherwise the Windows PowerShell host is used on Windows
 * and `pwsh` elsewhere. An unresolvable shell fails the call with the executable
 * it looked for instead of silently skipping the check.
 * @param environment - process environment.
 * @returns the executable name or absolute path.
 */
export function resolveShell(environment: Readonly<Record<string, string | undefined>> = process.env): string {
  const override = environment.NCAG_SHELL
  if (override !== undefined && override.length > 0) return override
  /* v8 ignore next -- Windows and POSIX lanes each cover their own default shell; the other is unreachable on the running host. */
  return process.platform === 'win32' ? (environment.PS_EXECUTABLE ?? 'powershell.exe') : DEFAULT_SHELL
}

/** Argument object accepted by the file tools. */
export interface FileArgs {
  /** Workspace-relative or absolute path inside the workspace. */
  readonly path: string
}

/** Argument object accepted by the edit tool. */
export interface EditArgs extends FileArgs {
  /** Exact text to replace; it must occur exactly once. */
  readonly oldText: string
  /** Replacement text. */
  readonly newText: string
}

/** Argument object accepted by the write tool. */
export interface WriteArgs extends FileArgs {
  /** Complete file content. */
  readonly content: string
}

/** Argument object accepted by the glob tool. */
export interface GlobArgs {
  /** Glob pattern relative to the workspace root. */
  readonly pattern: string
}

/** Argument object accepted by the grep tool. */
export interface GrepArgs {
  /** Regular expression source. */
  readonly pattern: string
  /** Optional file-name suffix filter, e.g. `.ts`. */
  readonly include?: string
}

/** Argument object accepted by the shell tools. */
export interface CommandArgs {
  /** Command text executed in the workspace root. */
  readonly command: string
}

/** One tool the runtime can execute, with its schema and body. */
export interface RuntimeTool {
  readonly schema: AgentToolSchema
  /**
   * Execute one call.
   *
   * Filesystem and shell tools answer synchronously; the coordination tools
   * return a promise because they wait for the orchestrator to run a helper or
   * record a handoff.
   * @param args - parsed, schema-validated arguments.
   * @param env - execution environment.
   * @returns the model-facing outcome, or a promise of it.
   */
  execute(args: unknown, env: ToolEnvironment): ToolOutcome | Promise<ToolOutcome>
}

/**
 * Resolve one path inside the workspace, refusing escapes.
 * @param workspacePath - absolute workspace root.
 * @param candidate - path supplied by the model.
 * @returns the absolute resolved path.
 * @throws Error when the path escapes the workspace.
 */
export function resolveInsideWorkspace(workspacePath: string, candidate: string): string {
  const root = resolve(workspacePath)
  const target = isAbsolute(candidate) ? resolve(candidate) : resolve(root, candidate)
  const rel = relative(root, target)
  if (rel !== '' && (rel.startsWith('..' + sep) || rel === '..' || isAbsolute(rel))) {
    throw new Error(`ncag: path ${JSON.stringify(candidate)} escapes the agent workspace`)
  }
  return target
}

/** Render one file with line numbers, as the read tool returns it. */
function numbered(text: string, from = 1): string {
  return text.split('\n').map((line, index) => `${index + from}: ${line}`).join('\n')
}

/** Truncate output to the budget, marking the omission explicitly. */
function clip(text: string, budget: number): string {
  if (text.length <= budget) return text
  return `${text.slice(0, budget)}\n… ${text.length - budget} more characters omitted`
}

const READ_SCHEMA: AgentToolSchema = {
  name: 'read',
  description: 'Read a file from the workspace. Returns numbered lines. Paths outside the workspace are refused.',
  parameters: { type: 'object', properties: { path: { type: 'string', description: 'Workspace-relative file path.' } }, required: ['path'], additionalProperties: false },
}

const WRITE_SCHEMA: AgentToolSchema = {
  name: 'write',
  description: 'Create or completely replace a file in the workspace. Missing parent directories are created.',
  parameters: { type: 'object', properties: { path: { type: 'string' }, content: { type: 'string', description: 'Complete file content.' } }, required: ['path', 'content'], additionalProperties: false },
}

const EDIT_SCHEMA: AgentToolSchema = {
  name: 'edit',
  description: 'Replace one exact occurrence of text in a workspace file. The call fails when the old text occurs zero or more than one times.',
  parameters: { type: 'object', properties: { path: { type: 'string' }, oldText: { type: 'string' }, newText: { type: 'string' } }, required: ['path', 'oldText', 'newText'], additionalProperties: false },
}

const GLOB_SCHEMA: AgentToolSchema = {
  name: 'glob',
  description: 'List workspace files whose path matches a glob pattern such as **/*.ts. Returns at most 200 paths.',
  parameters: { type: 'object', properties: { pattern: { type: 'string' } }, required: ['pattern'], additionalProperties: false },
}

const GREP_SCHEMA: AgentToolSchema = {
  name: 'grep',
  description: 'Search workspace file contents with a regular expression. Returns matching lines with file and line number, at most 200 matches.',
  parameters: { type: 'object', properties: { pattern: { type: 'string' }, include: { type: 'string', description: 'Optional file suffix filter such as .ts' } }, required: ['pattern'], additionalProperties: false },
}

const SHELL_SCHEMA: AgentToolSchema = {
  name: 'shell',
  description: 'Run a shell command in the workspace root and return combined output and the exit code. Use it to build, test and inspect the project.',
  parameters: { type: 'object', properties: { command: { type: 'string' } }, required: ['command'], additionalProperties: false },
}

/**
 * Parse the arguments of a tool call and require every named key to be a string.
 * The check runs on the model-supplied JSON, so a missing or non-string
 * argument fails the call instead of reaching the filesystem call.
 * @param raw - parsed JSON arguments.
 * @param keys - argument names the tool requires.
 * @returns a record whose named keys are strings.
 * @throws Error when the payload is not an object or a key is missing or mistyped.
 */
function argsOf<K extends string>(raw: unknown, keys: readonly K[]): Record<K, string> {
  if (raw === null || typeof raw !== 'object') throw new Error('ncag: tool arguments must be a JSON object')
  const record = raw as Record<string, unknown>
  const typed = {} as Record<K, string>
  for (const key of keys) {
    const value = record[key]
    if (typeof value !== 'string') throw new Error('ncag: tool argument ' + key + ' must be a string')
    typed[key] = value
  }
  return typed
}

/** Build the read tool. */
const readTool: RuntimeTool = {
  schema: READ_SCHEMA,
  execute(raw, env) {
    const args = argsOf(raw, ['path'] as const)
    const path = resolveInsideWorkspace(env.workspacePath, args.path)
    if (!existsSync(path)) return { text: `read failed: ${args.path} does not exist`, isError: true }
    const stat = statSync(path)
    if (stat.isDirectory()) return { text: `read failed: ${args.path} is a directory; use glob`, isError: true }
    const text = clip(readFileSync(path, 'utf8'), MAX_READ_BYTES)
    return { text: numbered(text), isError: false }
  },
}

/** Build the write tool. */
const writeTool: RuntimeTool = {
  schema: WRITE_SCHEMA,
  execute(raw, env) {
    const args = argsOf(raw, ['path', 'content'] as const)
    const path = resolveInsideWorkspace(env.workspacePath, args.path)
    mkdirSync(dirname(path), { recursive: true })
    writeFileSync(path, args.content)
    return { text: `wrote ${args.content.length} bytes to ${args.path}`, isError: false }
  },
}

/** Build the edit tool. */
const editTool: RuntimeTool = {
  schema: EDIT_SCHEMA,
  execute(raw, env) {
    const args = argsOf(raw, ['path', 'oldText', 'newText'] as const)
    const path = resolveInsideWorkspace(env.workspacePath, args.path)
    if (!existsSync(path)) return { text: `edit failed: ${args.path} does not exist`, isError: true }
    const before = readFileSync(path, 'utf8')
    const first = before.indexOf(args.oldText)
    if (first < 0) return { text: `edit failed: the old text does not occur in ${args.path}`, isError: true }
    if (before.indexOf(args.oldText, first + args.oldText.length) >= 0) {
      return { text: `edit failed: the old text occurs more than once in ${args.path}`, isError: true }
    }
    const after = before.slice(0, first) + args.newText + before.slice(first + args.oldText.length)
    writeFileSync(path, after)
    return { text: `edited ${args.path}`, isError: false }
  },
}

/** Convert one glob to a regular expression over workspace-relative paths. */
function globToRegExp(pattern: string): RegExp {
  let source = ''
  // Code-unit indexing is intentional here: a pattern is matched against file
  // paths, where a surrogate pair is part of one path segment rather than a
  // character boundary that must be preserved.
  const characters = Array.from(pattern, (_char, index) => pattern.slice(index, index + 1))
  for (const [index, char] of characters.entries()) {
    if (char === '*') {
      if (characters[index + 1] === '*') {
        source += '.*'
      } else {
        source += '[^/]*'
      }
    } else if (char === '?') {
      source += '[^/]'
    } else if ('\\^$.[]{}()*+?|/'.includes(char)) {
      source += '\\' + char
    } else {
      source += char
    }
  }
  return new RegExp(`^${source}$`)
}

/** Directory names never walked by discovery tools. */
const IGNORED_DIRECTORIES: ReadonlySet<string> = new Set(['node_modules', '.git', 'dist', 'build', 'lib', 'coverage', '.dsh-build'])

/**
 * Walk one workspace and collect relative file paths.
 * @param root - absolute workspace root.
 * @returns the relative paths of every file outside ignored directories.
 */
function walkFiles(root: string): string[] {
  const found: string[] = []
  const visit = (directory: string, prefix: string): void => {
    let entries
    try {
      entries = readdirSync(directory, { withFileTypes: true })
    } catch {
      return
    }
    for (const entry of entries) {
      const relativePath = prefix === '' ? entry.name : `${prefix}/${entry.name}`
      if (entry.isDirectory()) {
        if (!IGNORED_DIRECTORIES.has(entry.name)) visit(`${directory}${sep}${entry.name}`, relativePath)
      } else if (entry.isFile()) {
        found.push(relativePath)
      }
    }
  }
  visit(root, '')
  return found
}

/** Build the glob tool. */
const globTool: RuntimeTool = {
  schema: GLOB_SCHEMA,
  execute(raw, env) {
    const args = argsOf(raw, ['pattern'] as const)
    const matcher = globToRegExp(args.pattern)
    const matches = walkFiles(env.workspacePath).filter(path => matcher.test(path)).sort()
    if (matches.length === 0) return { text: `no files match ${args.pattern}`, isError: false }
    return { text: matches.slice(0, 200).join('\n'), isError: false }
  },
}

/** Build the grep tool. */
const grepTool: RuntimeTool = {
  schema: GREP_SCHEMA,
  execute(raw, env) {
    const args = argsOf(raw, ['pattern'] as const)
    let expression: RegExp
    try {
      expression = new RegExp(args.pattern)
    } catch (error: unknown) {
      return { text: `grep failed: ${(error as Error).message}`, isError: true }
    }
    const matches: string[] = []
    for (const relativePath of walkFiles(env.workspacePath)) {
      const include = typeof (raw as Record<string, unknown>).include === 'string' ? (raw as Record<string, string>).include : undefined
      if (include !== undefined && !relativePath.endsWith(include)) continue
      let text: string
      try {
        text = readFileSync(`${env.workspacePath}${sep}${relativePath}`, 'utf8')
      } catch {
        /* v8 ignore next -- a file can disappear between the walk and the read; skipping it is the documented race handling. */
        continue
      }
      const lines = text.split('\n')
      for (const [index, line] of lines.entries()) {
        if (expression.test(line)) {
          matches.push(`${relativePath}:${index + 1}: ${line.trim().slice(0, 200)}`)
          if (matches.length >= 200) return { text: matches.join('\n'), isError: false }
        }
      }
    }
    return { text: matches.length === 0 ? `no matches for ${args.pattern}` : matches.join('\n'), isError: false }
  },
}

/** Build the shell tool. */
const shellTool: RuntimeTool = {
  schema: SHELL_SCHEMA,
  execute(raw, env) {
    const args = argsOf(raw, ['command'] as const)
    try {
      const shell = resolveShell()
      const output = execFileSync(shell, ['-NoProfile', '-NonInteractive', '-Command', args.command], {
        cwd: env.workspacePath,
        timeout: COMMAND_TIMEOUT_MS,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
        windowsHide: true,
        maxBuffer: 8 * 1024 * 1024,
      })
      return { text: clip(`exit 0\n${output}`, MAX_COMMAND_OUTPUT_BYTES), isError: false }
    } catch (error: unknown) {
      const failure = error as { stdout?: string; stderr?: string; status?: number; message?: string }
      const status = typeof failure.status === 'number' ? failure.status : 'unknown'
      const combined = `${failure.stdout ?? ''}${failure.stderr ?? ''}`
      /* v8 ignore next -- a spawn failure always carries a message; the last fallback is unreachable defense. */
      return { text: clip(`exit ${status}\n${combined || failure.message || 'command failed'}`, MAX_COMMAND_OUTPUT_BYTES), isError: true }
    }
  },
}

/** Tool names that carry an agent's coordination requests to the orchestrator. */
export const REQUEST_HELP_TOOL = 'request_help'

/** Tool name that hands one task to another agent through the orchestrator. */
export const HAND_OFF_TOOL = 'hand_off'

const HELP_SCHEMA: AgentToolSchema = {
  name: REQUEST_HELP_TOOL,
  description: 'Ask the orchestrator for a specialist to help with the current task. A real helper agent runs with the context you supply, and its answer comes back as the tool result. Use it when the task needs expertise you do not have; do not use it to delegate the whole task.',
  parameters: {
    type: 'object',
    properties: {
      problem: { type: 'string', description: 'What you cannot resolve, stated concretely.' },
      capability: { type: 'string', description: 'Capability of the specialist needed, for example security-review. Empty lets the orchestrator choose.' },
      urgency: { type: 'string', description: 'low, normal, high or critical.' },
      context: { type: 'string', description: 'Evidence the helper needs: paths, commands already run, and their output.' },
      expectedAssistance: { type: 'string', description: 'What the helper should produce.' },
      newHelperRequired: { type: 'string', description: 'true when a new specialist instance must be spawned rather than an existing one consulted.' },
    },
    required: ['problem', 'expectedAssistance'],
    additionalProperties: false,
  },
}

const HANDOFF_SCHEMA: AgentToolSchema = {
  name: HAND_OFF_TOOL,
  description: 'Hand the current task to another agent. The platform records the package, selects the receiving profile, transfers the workspace and releases the late resources only after the handoff is durable. Use it when another specialization must continue the work.',
  parameters: {
    type: 'object',
    properties: {
      capability: { type: 'string', description: 'Capability of the agent that must continue the work.' },
      completedWork: { type: 'string', description: 'What is finished, with the files and commands that prove it.' },
      remainingWork: { type: 'string', description: 'What still has to happen.' },
      knownProblems: { type: 'string', description: 'Problems the next agent must not rediscover.' },
      outstandingQuestions: { type: 'string', description: 'Questions the next agent must answer or escalate.' },
    },
    required: ['capability', 'completedWork', 'remainingWork'],
    additionalProperties: false,
  },
}

/** Normalize one urgency argument, refusing an unknown value. */
function urgencyOf(value: string | undefined): HelpRequest['urgency'] {
  const kinds: readonly HelpRequest['urgency'][] = ['low', 'normal', 'high', 'critical']
  return kinds.find(kind => kind === value) ?? 'normal'
}

/** Ask the orchestrator for help, reporting a missing coordination surface. */
const helpTool: RuntimeTool = {
  schema: HELP_SCHEMA,
  execute(raw, env): ToolOutcome | Promise<ToolOutcome> {
    const args = argsOf(raw, ['problem', 'expectedAssistance'] as const)
    const optional = argsObject(raw)
    const coordination = env.coordination
    if (coordination === undefined) {
      return { text: 'no orchestrator is attached to this run, so no helper can be requested', isError: true }
    }
    return coordination.requestHelp({
      problem: args.problem,
      capability: typeof optional.capability === 'string' ? optional.capability : '',
      urgency: urgencyOf(typeof optional.urgency === 'string' ? optional.urgency : undefined),
      context: typeof optional.context === 'string' ? optional.context : '',
      expectedAssistance: args.expectedAssistance,
      newHelperRequired: optional.newHelperRequired === 'true' || optional.newHelperRequired === true,
    })
  },
}

/** Hand the current task to another agent, reporting a missing coordination surface. */
const handOffTool: RuntimeTool = {
  schema: HANDOFF_SCHEMA,
  execute(raw, env): ToolOutcome | Promise<ToolOutcome> {
    const args = argsOf(raw, ['capability', 'completedWork', 'remainingWork'] as const)
    const optional = argsObject(raw)
    const coordination = env.coordination
    if (coordination === undefined) {
      return { text: 'no orchestrator is attached to this run, so no handoff can be recorded', isError: true }
    }
    const request: HandoffRequest = {
      capability: args.capability,
      completedWork: args.completedWork,
      remainingWork: args.remainingWork,
      knownProblems: typeof optional.knownProblems === 'string' ? optional.knownProblems : '',
      outstandingQuestions: typeof optional.outstandingQuestions === 'string' ? optional.outstandingQuestions : '',
    }
    return coordination.handOff(request)
  },
}

/** Read one tool's raw argument record without requiring any key. */
function argsObject(raw: unknown): Record<string, unknown> {
  if (raw === null || typeof raw !== 'object') throw new Error('ncag: tool arguments must be a JSON object')
  return raw as Record<string, unknown>
}

/** Every tool the runtime can execute, by name. */
export const RUNTIME_TOOLS: ReadonlyMap<string, RuntimeTool> = new Map([
  ['read', readTool],
  ['write', writeTool],
  ['edit', editTool],
  ['glob', globTool],
  ['grep', grepTool],
  ['shell', shellTool],
  [REQUEST_HELP_TOOL, helpTool],
  [HAND_OFF_TOOL, handOffTool],
])

/** Profile tool names that select the shell tool; the runtime owns one configured shell. */
const SHELL_ALIASES: ReadonlyMap<string, string> = new Map([
  ['pwsh', 'shell'],
  ['bash', 'shell'],
])

/**
 * Select the tools one profile may use.
 * @param toolNames - profile tool names.
 * @returns the registered tools the profile may execute.
 * @throws Error when a name names no tool and no alias.
 */
export function toolsForProfile(toolNames: readonly string[]): RuntimeTool[] {
  const selected: RuntimeTool[] = []
  for (const name of toolNames) {
    const tool = RUNTIME_TOOLS.get(SHELL_ALIASES.get(name) ?? name)
    if (tool === undefined) throw new Error(`ncag: profile requests unknown tool ${JSON.stringify(name)}`)
    if (!selected.some(entry => entry === tool)) selected.push(tool)
  }
  return selected
}
