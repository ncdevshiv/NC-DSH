/** Inverse-replay restore behavior over discarded turn slices. */

import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import type { SessionEvent } from '@deepseek-ai/dsh-session'
import { apply, EMPTY_TURN_RESTORE_REPORT, TurnRestore, workspacePath } from '../src/index.ts'

let cwd: string

beforeEach(async () => {
  cwd = await mkdtemp(path.join(tmpdir(), 'dsh-turn-restore-'))
})

afterEach(async () => {
  await rm(cwd, { recursive: true, force: true })
})

let seq = 0
function event(type: string, data: object): SessionEvent {
  seq += 1
  return { type: type as SessionEvent['type'], seq, time: 0, data: data as never } as SessionEvent
}

function toolCall(callId: string, name: string): SessionEvent {
  return event('tool/call', { turn: 1, step: 1, callId, name, arguments: '{}' })
}

function toolResult(callId: string, meta: unknown): SessionEvent {
  return event('tool/result', {
    turn: 1,
    step: 1,
    message: {
      role: 'user',
      content: [{ type: 'text', text: 'ok' }],
      source: { kind: 'tool', callId },
    },
    meta,
  })
}

function basis(b: { path: string; op: 'create' | 'update' | 'edit'; before: string | null; after: string | null }): unknown {
  return { diffs: [], basis: b }
}

describe('TurnRestore.restore', () => {
  it('rewrites an updated file to its pre-turn text', async () => {
    const file = path.join(cwd, 'log.md')
    await writeFile(file, 'new', 'utf8')
    const restore = new TurnRestore()
    const report = await restore.restore({
      cwd,
      events: [
        event('turn/start', { turn: 1 }),
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: 'log.md', op: 'update', before: 'old', after: 'new' })),
      ],
    })
    expect(report.restored).toBe(1)
    expect(await readFile(file, 'utf8')).toBe('old')
  })

  it('reports a created file whose content changed as a conflict', async () => {
    const file = path.join(cwd, 'new.md')
    await writeFile(file, 'user-changed', 'utf8')
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: 'new.md', op: 'create', before: null, after: 'hello' })),
      ],
    })
    expect(report.conflicts).toEqual(['new.md'])
    expect(await readFile(file, 'utf8')).toBe('user-changed')
  })

  it('reports an update whose file is already missing as a conflict', async () => {
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: 'gone.md', op: 'update', before: 'old', after: 'new' })),
      ],
    })
    expect(report.conflicts).toEqual(['gone.md'])
    expect(report.restored).toBe(0)
  })

  // A read-only directory makes the restore write fail: the plan must fold
  // into a reported conflict. Windows read-only flags do not block writes the
  // way POSIX mode 0555 does, so this case runs on POSIX (CI) only.
  it.runIf(process.platform !== 'win32')(
    'reports a write failure as a conflict instead of aborting the pass',
    async () => {
      const locked = path.join(cwd, 'locked')
      await mkdir(locked)
      const file = path.join(locked, 'log.md')
      await writeFile(file, 'new', 'utf8')
      await chmod(locked, 0o555)
      try {
        const report = await new TurnRestore().restore({
          cwd,
          events: [
            toolCall('c1', 'write'),
            toolResult('c1', basis({ path: 'locked/log.md', op: 'update', before: 'old', after: 'new' })),
          ],
        })
        expect(report.conflicts).toEqual(['locked/log.md'])
        expect(report.restored).toBe(0)
        expect(await readFile(file, 'utf8')).toBe('new')
      } finally {
        await chmod(locked, 0o755)
      }
    },
  )

  it('steps back through repeated edits newest-first', async () => {
    const file = path.join(cwd, 'a.ts')
    await writeFile(file, 'new', 'utf8')
    const events = [
      toolCall('c1', 'edit'),
      toolResult('c1', basis({ path: 'a.ts', op: 'edit', before: 'old', after: 'mid' })),
      toolCall('c2', 'edit'),
      toolResult('c2', basis({ path: 'a.ts', op: 'edit', before: 'mid', after: 'new' })),
    ]
    const report = await new TurnRestore().restore({ cwd, events })
    expect(report.restored).toBe(2)
    expect(await readFile(file, 'utf8')).toBe('old')
  })

  it('deletes a created file whose content still matches', async () => {
    const file = path.join(cwd, 'new.md')
    await writeFile(file, 'hello', 'utf8')
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: 'new.md', op: 'create', before: null, after: 'hello' })),
      ],
    })
    expect(report.restored).toBe(1)
    await expect(readFile(file, 'utf8')).rejects.toThrow()
  })

  it('counts a create as pre-state when the file is already missing', async () => {
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: 'gone.md', op: 'create', before: null, after: 'hello' })),
      ],
    })
    expect(report.restored).toBe(1)
    expect(report.conflicts).toEqual([])
  })

  it('reports a conflict instead of clobbering an intervening user edit', async () => {
    const file = path.join(cwd, 'log.md')
    await writeFile(file, 'user-modified', 'utf8')
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: 'log.md', op: 'update', before: 'old', after: 'new' })),
      ],
    })
    expect(report.conflicts).toEqual(['log.md'])
    expect(await readFile(file, 'utf8')).toBe('user-modified')
  })

  it('reports a size-capped write as not restorable', async () => {
    const file = path.join(cwd, 'big.ts')
    await writeFile(file, 'huge', 'utf8')
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: 'big.ts', op: 'update', before: null, after: null })),
      ],
    })
    expect(report.notRestorable).toEqual({ count: 1, toolNames: [] })
    expect(await readFile(file, 'utf8')).toBe('huge')
  })

  it('counts basis-less shell and editor tools without restoring anything', async () => {
    const shell = path.join(cwd, 'shell.out')
    await writeFile(shell, 'ran', 'utf8')
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('s1', 'bash'),
        toolResult('s1', { diffs: [] }),
        toolCall('s2', 'str_replace_editor'),
        toolResult('s2', undefined),
        // An orphan result (no paired call) names no tool and counts nothing.
        toolResult('lost', { diffs: [] }),
      ],
    })
    expect(report.shell).toEqual({ count: 1, names: ['bash'] })
    expect(report.notRestorable).toEqual({ count: 1, toolNames: ['str_replace_editor'] })
    expect(report.restored).toBe(0)
  })

  it('reports a create without a recorded after as not restorable', async () => {
    const file = path.join(cwd, 'huge.md')
    await writeFile(file, 'megabytes', 'utf8')
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: 'huge.md', op: 'create', before: null, after: null })),
      ],
    })
    expect(report.notRestorable).toEqual({ count: 1, toolNames: [] })
    expect(await readFile(file, 'utf8')).toBe('megabytes')
  })

  it('refuses paths that escape the workspace root', async () => {
    const report = await new TurnRestore().restore({
      cwd,
      events: [
        toolCall('c1', 'write'),
        toolResult('c1', basis({ path: '../escape.txt', op: 'update', before: 'x', after: 'y' })),
      ],
    })
    expect(report.conflicts).toEqual(['../escape.txt'])
  })

  it('resolves absolute paths only under the root', () => {
    expect(workspacePath(cwd, 'a/b.ts')).toBe(path.join(cwd, 'a', 'b.ts'))
    expect(workspacePath(cwd, '.')).toBe(cwd)
    expect(workspacePath(cwd, path.join(cwd, 'in.ts'))).toBe(path.join(cwd, 'in.ts'))
    expect(workspacePath(cwd, '../up.ts')).toBeNull()
    expect(workspacePath(cwd, '/etc/passwd')).toBeNull()
  })

  it('interleaves files and keeps per-file ordering', async () => {
    const a = path.join(cwd, 'a.md')
    const b = path.join(cwd, 'b.md')
    await writeFile(a, 'a2', 'utf8')
    await writeFile(b, 'b2', 'utf8')
    const events = [
      toolCall('c1', 'write'),
      toolResult('c1', basis({ path: 'a.md', op: 'update', before: 'a0', after: 'a1' })),
      toolCall('c2', 'write'),
      toolResult('c2', basis({ path: 'b.md', op: 'update', before: 'b0', after: 'b2' })),
      toolCall('c3', 'write'),
      toolResult('c3', basis({ path: 'a.md', op: 'update', before: 'a1', after: 'a2' })),
    ]
    const report = await new TurnRestore().restore({ cwd, events })
    expect(report.restored).toBe(3)
    expect(await readFile(a, 'utf8')).toBe('a0')
    expect(await readFile(b, 'utf8')).toBe('b0')
  })

  it('registers the turnRestore service on ctx when applied', async () => {
    const ctx = new Context()
    apply(ctx)
    expect(ctx.get('turnRestore')).toBeInstanceOf(TurnRestore)
    await ctx.fiber.dispose()
  })

  it('exports an all-zero report for skip reasons', () => {
    expect(EMPTY_TURN_RESTORE_REPORT).toEqual({
      restored: 0,
      conflicts: [],
      notRestorable: { count: 0, toolNames: [] },
      shell: { count: 0, names: [] },
    })
  })
})
