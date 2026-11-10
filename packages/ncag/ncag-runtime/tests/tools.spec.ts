import { mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  HAND_OFF_TOOL, MAX_READ_BYTES, REQUEST_HELP_TOOL, RUNTIME_TOOLS, resolveInsideWorkspace, resolveShell, toolsForProfile,
} from '../src/tools.ts'
import type { ToolEnvironment, ToolOutcome } from '../src/ports.ts'

let root: string

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'ncag-tools-'))
})

afterEach(() => {
  rmSync(root, { recursive: true, force: true })
})

function environment(overrides: Partial<ToolEnvironment> = {}): ToolEnvironment {
  return {
    workspacePath: root,
    agentId: 'agt_1',
    taskId: null,
    projectId: 'prj_1',
    signal: new AbortController().signal,
    ...overrides,
  }
}

function tool(name: string) {
  const found = RUNTIME_TOOLS.get(name)
  if (found === undefined) throw new Error(`missing tool ${name}`)
  return found
}

/**
 * Run one tool call that cannot block on the orchestrator.
 *
 * The filesystem and shell tools answer synchronously; the coordination tools
 * answer through a promise, and the specs exercise the synchronous ones here.
 * @param name - tool name.
 * @param args - parsed arguments.
 * @param env - execution environment.
 * @returns the synchronous outcome.
 * @throws Error when the tool answered through a promise instead.
 */
function runSync(name: string, args: unknown, env: ToolEnvironment): ToolOutcome {
  const outcome = tool(name).execute(args, env)
  if (outcome instanceof Promise) throw new Error(`tool ${name} answered asynchronously; await it instead`)
  return outcome
}

describe('ncag runtime tools', () => {
  it('resolves the shell from the override, the host platform and its fallback', () => {
    expect(resolveShell({ NCAG_SHELL: '/bin/sh' })).toBe('/bin/sh')
    const host = process.platform === 'win32'
      ? resolveShell({ PS_EXECUTABLE: 'ps.exe' })
      : resolveShell({})
    expect(typeof host).toBe('string')
    expect(resolveShell({})).toBe(process.platform === 'win32'
      ? (process.env.PS_EXECUTABLE ?? 'powershell.exe')
      : 'pwsh')
  })

  it('refuses paths that escape the workspace', () => {
    expect(resolveInsideWorkspace(root, 'a/b.txt')).toBe(join(root, 'a', 'b.txt'))
    expect(resolveInsideWorkspace(root, join(root, 'inner.txt'))).toBe(join(root, 'inner.txt'))
    // The workspace root itself is inside the workspace.
    expect(resolveInsideWorkspace(root, '.')).toBe(root)
    expect(() => resolveInsideWorkspace(root, '..')).toThrow(/escapes the agent workspace/)
    expect(() => resolveInsideWorkspace(root, join(root, '..', 'sibling'))).toThrow(/escapes the agent workspace/)
    expect(() => resolveInsideWorkspace(root, 'nested/../../outside')).toThrow(/escapes the agent workspace/)
  })

  it('reads a file with line numbers, refuses a missing path and a directory, and clips a large file', () => {
    writeFileSync(join(root, 'a.txt'), 'one\ntwo\n')
    expect(runSync('read', { path: 'a.txt' }, environment()).text).toBe('1: one\n2: two\n3: ')
    expect(runSync('read', { path: 'missing.txt' }, environment())).toEqual({ text: 'read failed: missing.txt does not exist', isError: true })
    mkdirSync(join(root, 'sub'))
    expect(runSync('read', { path: 'sub' }, environment())).toEqual({ text: 'read failed: sub is a directory; use glob', isError: true })
    writeFileSync(join(root, 'big.txt'), 'x'.repeat(MAX_READ_BYTES + 10))
    const clipped = runSync('read', { path: 'big.txt' }, environment()).text
    expect(clipped).toContain('more characters omitted')
  })

  it('rejects malformed tool arguments', () => {
    const read = tool('read')
    expect(() => read.execute(null, environment())).toThrow(/must be a JSON object/)
    expect(() => read.execute({ path: 3 }, environment())).toThrow(/tool argument path must be a string/)
    expect(() => read.execute({}, environment())).toThrow(/tool argument path must be a string/)
  })

  it('writes a file and creates its parent directories', async () => {
    const write = tool('write')
    const outcome = await write.execute({ path: 'deep/nested/a.txt', content: 'hello' }, environment())
    expect(outcome).toEqual({ text: 'wrote 5 bytes to deep/nested/a.txt', isError: false })
    expect((await tool('read').execute({ path: 'deep/nested/a.txt' }, environment())).text).toBe('1: hello')
  })

  it('edits exactly one occurrence and refuses zero or many', async () => {
    writeFileSync(join(root, 'e.txt'), 'alpha beta gamma')
    const edit = tool('edit')
    expect(await edit.execute({ path: 'e.txt', oldText: 'beta', newText: 'BETA' }, environment())).toEqual({ text: 'edited e.txt', isError: false })
    expect(await edit.execute({ path: 'e.txt', oldText: 'missing', newText: 'x' }, environment())).toEqual({ text: 'edit failed: the old text does not occur in e.txt', isError: true })
    expect(await edit.execute({ path: 'e.txt', oldText: 'a', newText: 'x' }, environment())).toEqual({ text: 'edit failed: the old text occurs more than once in e.txt', isError: true })
    expect(await edit.execute({ path: 'absent.txt', oldText: 'a', newText: 'b' }, environment())).toEqual({ text: 'edit failed: absent.txt does not exist', isError: true })
  })

  it('globs workspace files, honours wildcards and skips ignored directories', async () => {
    mkdirSync(join(root, 'src'))
    mkdirSync(join(root, 'node_modules'))
    writeFileSync(join(root, 'src', 'a.ts'), 'a')
    writeFileSync(join(root, 'src', 'b.md'), 'b')
    writeFileSync(join(root, 'node_modules', 'c.ts'), 'c')
    const glob = tool('glob')
    expect((await glob.execute({ pattern: '**/*.ts' }, environment())).text).toBe('src/a.ts')
    expect((await glob.execute({ pattern: 'src/?.ts' }, environment())).text).toBe('src/a.ts')
    expect((await glob.execute({ pattern: 'src/*' }, environment())).text).toBe('src/a.ts\nsrc/b.md')
    expect(await glob.execute({ pattern: '**/*.zzz' }, environment())).toEqual({ text: 'no files match **/*.zzz', isError: false })
  })

  it('returns no matches when the workspace cannot be walked and skips non-file entries', async () => {
    const glob = tool('glob')
    expect(await glob.execute({ pattern: '*' }, environment({ workspacePath: join(root, 'missing') }))).toEqual({ text: 'no files match *', isError: false })
    writeFileSync(join(root, 'target.txt'), 'x')
    // A link reports as neither a directory nor a file, so the walk skips it.
    mkdirSync(join(root, 'subdir'))
    writeFileSync(join(root, 'subdir', 'inner.txt'), 'x')
    symlinkSync(join(root, 'subdir'), join(root, 'linked'), process.platform === 'win32' ? 'junction' : 'dir')
    expect((await glob.execute({ pattern: '**/*.txt' }, environment())).text).toBe('subdir/inner.txt')
    expect((await glob.execute({ pattern: '*.txt' }, environment())).text).toBe('target.txt')
  })

  it('caps grep output at two hundred matches', () => {
    writeFileSync(join(root, 'many.txt'), Array.from({ length: 250 }, () => 'needle').join('\n'))
    const output = runSync('grep', { pattern: 'needle' }, environment())
    expect(output.isError).toBe(false)
    expect(output.text.split('\n')).toHaveLength(200)
  })

  it('greps file contents with an optional suffix filter and reports an invalid expression', async () => {
    mkdirSync(join(root, 'src'))
    writeFileSync(join(root, 'src', 'a.ts'), 'const value = 1\nother\n')
    writeFileSync(join(root, 'src', 'b.md'), 'const value = 2\n')
    const grep = tool('grep')
    expect((await grep.execute({ pattern: 'value', include: '.ts' }, environment())).text).toBe('src/a.ts:1: const value = 1')
    expect((await grep.execute({ pattern: 'value' }, environment())).text).toContain('src/b.md:1: const value = 2')
    expect(await grep.execute({ pattern: 'absent' }, environment())).toEqual({ text: 'no matches for absent', isError: false })
    expect((await grep.execute({ pattern: '(' }, environment())).isError).toBe(true)
  })

  it('runs a shell command in the workspace and reports its failure', async () => {
    const shell = tool('shell')
    const ok = await shell.execute({ command: 'Write-Output ncag-shell-ok' }, environment())
    expect(ok.isError).toBe(false)
    expect(ok.text).toContain('ncag-shell-ok')
    const failed = await shell.execute({ command: 'exit 3' }, environment())
    expect(failed.isError).toBe(true)
    expect(failed.text).toContain('exit 3')
    // An unresolvable shell executable reports the failure instead of skipping it.
    const original = process.env.NCAG_SHELL
    process.env.NCAG_SHELL = join(root, 'no-such-shell')
    try {
      const unresolved = resolveShell()
      expect(unresolved).toBe(join(root, 'no-such-shell'))
      const spawnFailure = await shell.execute({ command: 'Write-Output x' }, environment())
      expect(spawnFailure.isError).toBe(true)
      expect(spawnFailure.text).toContain('exit unknown')
    } finally {
      if (original === undefined) delete process.env.NCAG_SHELL
      else process.env.NCAG_SHELL = original
    }
  })

  it('selects the tools a profile declares and refuses an unknown one', () => {
    expect(toolsForProfile(['read', 'write']).map(entry => entry.schema.name)).toEqual(['read', 'write'])
    // pwsh and bash alias the runtime's single configured shell.
    expect(toolsForProfile(['pwsh', 'bash']).map(entry => entry.schema.name)).toEqual(['shell'])
    expect(toolsForProfile(['request_help', 'hand_off']).map(entry => entry.schema.name))
      .toEqual(['request_help', 'hand_off'])
    expect(() => toolsForProfile(['nope'])).toThrow(/unknown tool/)
  })

  it('sends a help request through the coordination surface and reports a missing one', async () => {
    const request = { problem: 'unsafe retry', capability: 'critique', urgency: 'high', context: 'src/a.ts', expectedAssistance: 'a verdict', newHelperRequired: false }
    const outcome = await tool(REQUEST_HELP_TOOL).execute(
      { ...request, newHelperRequired: 'false' },
      environment({
        coordination: {
          requestHelp: call => Promise.resolve({ text: `helper saw ${call.problem}`, isError: false }),
          handOff: () => Promise.resolve({ text: 'unused', isError: false }),
        },
      }),
    )
    expect(outcome).toEqual({ text: 'helper saw unsafe retry', isError: false })
    const missing = await tool(REQUEST_HELP_TOOL).execute(
      { problem: 'p', expectedAssistance: 'a' },
      environment(),
    )
    expect(missing.isError).toBe(true)
    expect(missing.text).toContain('no orchestrator is attached')
    // An unknown urgency falls back to normal rather than reaching the orchestrator.
    const unknownUrgency = await tool(REQUEST_HELP_TOOL).execute(
      { problem: 'p', expectedAssistance: 'a', urgency: 'sometime' },
      environment({
        coordination: {
          requestHelp: call => Promise.resolve({ text: `urgency ${call.urgency}`, isError: false }),
          handOff: () => Promise.resolve({ text: 'unused', isError: false }),
        },
      }),
    )
    expect(unknownUrgency.text).toBe('urgency normal')
  })

  it('sends a handoff package through the coordination surface', async () => {
    const outcome = await tool(HAND_OFF_TOOL).execute(
      { capability: 'backend-implementation', completedWork: 'wrote a.txt', remainingWork: 'finish b.txt', knownProblems: 'none' },
      environment({
        coordination: {
          requestHelp: () => Promise.resolve({ text: 'unused', isError: false }),
          handOff: request => Promise.resolve({ text: `next owner ${request.capability} keeps ${request.completedWork}`, isError: false }),
        },
      }),
    )
    expect(outcome.text).toContain('backend-implementation')
    expect(outcome.text).toContain('wrote a.txt')
    const missing = await tool(HAND_OFF_TOOL).execute(
      { capability: 'backend-implementation', completedWork: 'c', remainingWork: 'r' },
      environment(),
    )
    expect(missing.isError).toBe(true)
    expect(missing.text).toContain('no handoff can be recorded')
  })
})
