import { Context } from '@deepseek-ai/cordis'
import { createHash } from 'node:crypto'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { FsError, FsTargetKey, FsVersion, type FsTarget } from '@deepseek-ai/dsh-fs'
import { afterEach, describe, expect, it } from 'vitest'
import { NcToolsFileSystem } from '../src/index.ts'

const FIXTURE = fileURLToPath(new URL('./fixture-nc-tools-server.mjs', import.meta.url))
const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0)) {
    rmSync(root, { recursive: true, force: true })
  }
})

/** A served workspace with a fresh directory for one test. */
function workspace(): string {
  const root = mkdtempSync(join(tmpdir(), 'dsh-fs-mcp-'))
  roots.push(root)
  return root
}

/** A provider wired to the fixture server over the given workspace. */
async function setup(root: string): Promise<{ fs: Context['fs']; dispose: () => Promise<void> }> {
  const ctx = new Context()
  const fiber = await ctx.plugin(NcToolsFileSystem, {
    command: process.execPath,
    args: [FIXTURE, root],
    workspaceRoot: root,
  })
  return {
    fs: ctx.fs,
    dispose: async () => { await fiber.dispose() },
  }
}

/** The sha256 of a string's UTF-8 bytes. */
function sha256(text: string): string {
  return createHash('sha256').update(Buffer.from(text, 'utf8')).digest('hex')
}

describe('nc-tools filesystem provider', () => {
  it('reads a file through the server', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'hello\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      expect(await fs.readText(target)).toBe('hello\n')
    } finally {
      await dispose()
    }
  })

  it('reports a file as present with its content digest as the version', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'hello\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      const info = await fs.stat(target)
      expect(info?.type).toBe('file')
      expect(info?.size).toBe(6)
      expect(String(info?.version)).toBe(sha256('hello\n'))
    } finally {
      await dispose()
    }
  })

  it('reports an absent file as absent', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('nope.txt')
      expect(await fs.stat(target)).toBeUndefined()
    } finally {
      await dispose()
    }
  })

  it('reports a directory as a directory', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('.')
      const info = await fs.stat(target)
      expect(info?.type).toBe('directory')
    } finally {
      await dispose()
    }
  })

  it('writes a file and reports the create', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('new.txt')
      const outcome = await fs.writeText(target, 'written\n')
      expect(outcome.operation).toBe('create')
      expect(outcome.before).toBeNull()
      expect(outcome.after).toBe('written\n')
      expect(String(outcome.version)).toBe(sha256('written\n'))
      expect(readFileSync(join(root, 'new.txt'), 'utf8')).toBe('written\n')
    } finally {
      await dispose()
    }
  })

  it('replaces a file and reports the prior content', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'old\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      const outcome = await fs.writeText(target, 'new\n')
      expect(outcome.operation).toBe('update')
      expect(outcome.before).toBe('old\n')
      expect(outcome.after).toBe('new\n')
    } finally {
      await dispose()
    }
  })

  it('refuses a create over an existing file', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'old\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      await expect(fs.writeText(target, 'new\n', { kind: 'createIfAbsent' }))
        .rejects.toMatchObject({ code: 'FS_NOT_OBSERVED' })
    } finally {
      await dispose()
    }
  })

  it('refuses a replace against a stale version', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'old\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      await expect(fs.writeText(target, 'new\n', { kind: 'replaceIfVersion', version: FsVersion('not-the-digest') }))
        .rejects.toMatchObject({ code: 'FS_STALE_VERSION' })
    } finally {
      await dispose()
    }
  })

  it('replaces against the current version', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'old\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      const info = await fs.stat(target)
      const outcome = await fs.writeText(target, 'new\n', { kind: 'replaceIfVersion', version: info!.version })
      expect(outcome.operation).toBe('update')
      expect(outcome.after).toBe('new\n')
    } finally {
      await dispose()
    }
  })

  it('edits a literal and reports the new content', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'one two three\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      const outcome = await fs.editText(target, { oldString: 'two', newString: 'TWO', replaceAll: false })
      expect(outcome.before).toBe('one two three\n')
      expect(outcome.after).toBe('one TWO three\n')
      expect(readFileSync(join(root, 'a.txt'), 'utf8')).toBe('one TWO three\n')
    } finally {
      await dispose()
    }
  })

  it('replaces every occurrence when asked', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'x x x\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      const outcome = await fs.editText(target, { oldString: 'x', newString: 'y', replaceAll: true })
      expect(outcome.after).toBe('y y y\n')
    } finally {
      await dispose()
    }
  })

  it('refuses an edit whose text is not there', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'one\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      await expect(fs.editText(target, { oldString: 'absent', newString: 'x', replaceAll: false }))
        .rejects.toMatchObject({ code: 'FS_EDIT_NOT_FOUND' })
    } finally {
      await dispose()
    }
  })

  it('refuses an ambiguous edit', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'dup dup\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      await expect(fs.editText(target, { oldString: 'dup', newString: 'x', replaceAll: false }))
        .rejects.toMatchObject({ code: 'FS_AMBIGUOUS_EDIT' })
    } finally {
      await dispose()
    }
  })

  it('refuses an edit against a stale version', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'one\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      await expect(fs.editText(target, { oldString: 'one', newString: 'two', replaceAll: false }, { version: FsVersion('stale') }))
        .rejects.toMatchObject({ code: 'FS_STALE_VERSION' })
    } finally {
      await dispose()
    }
  })

  it('lists a directory with resolved child targets', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'a', 'utf8')
    writeFileSync(join(root, 'b.txt'), 'bb', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('.')
      const entries = await fs.listDir(target)
      expect(entries.map(entry => entry.name).sort()).toEqual(['a.txt', 'b.txt'])
      const first = entries.find(entry => entry.name === 'a.txt')
      expect(first?.type).toBe('file')
      expect(first?.size).toBe(1)
      expect(await fs.readText(first!.target)).toBe('a')
    } finally {
      await dispose()
    }
  })

  it('reads raw bytes exactly', async () => {
    const root = workspace()
    writeFileSync(join(root, 'bin.dat'), Buffer.from([0x00, 0xFF, 0xFE, 0x41]))
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('bin.dat')
      const bytes = await fs.readBytes(target, undefined, 1024)
      expect([...bytes]).toEqual([0x00, 0xFF, 0xFE, 0x41])
    } finally {
      await dispose()
    }
  })

  it('refuses a byte read over the limit instead of truncating it', async () => {
    const root = workspace()
    writeFileSync(join(root, 'bin.dat'), Buffer.from([1, 2, 3, 4, 5]))
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('bin.dat')
      await expect(fs.readBytes(target, undefined, 2)).rejects.toMatchObject({ code: 'FS_TOO_LARGE' })
    } finally {
      await dispose()
    }
  })

  it('reads one byte window', async () => {
    const root = workspace()
    writeFileSync(join(root, 'bin.dat'), Buffer.from([10, 20, 30, 40, 50]))
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('bin.dat')
      const window = await fs.readByteRange(target, { offset: 1, length: 2 })
      expect([...window]).toEqual([20, 30])
    } finally {
      await dispose()
    }
  })

  it('streams text in chunks', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'streamed\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      const chunks: string[] = []
      for await (const chunk of await fs.streamText(target)) chunks.push(chunk)
      expect(chunks.join('')).toBe('streamed\n')
    } finally {
      await dispose()
    }
  })

  it('resolves relative paths against the workspace and keeps absolute ones', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const relative = await fs.resolve('a.txt')
      expect(fs.processPath(relative)).toBe(join(root, 'a.txt'))
      const absolute = await fs.resolve(join(root, 'b.txt'))
      expect(fs.processPath(absolute)).toBe(join(root, 'b.txt'))
    } finally {
      await dispose()
    }
  })

  it('answers containment, file URLs and host paths from the target identity', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const parent = await fs.resolve('.')
      const child = await fs.resolve('a.txt')
      expect(fs.contains(parent, child)).toBe(true)
      expect(fs.contains(child, parent)).toBe(false)
      expect(fs.fileUrl(child)).toContain('file:///')
      expect(fs.processPathFromHostPath(join(root, 'a.txt'))).toBe(join(root, 'a.txt'))
      expect(fs.processPathFromHostPath(join(root, '..', 'elsewhere', 'a.txt'))).toBeUndefined()
    } finally {
      await dispose()
    }
  })

  it('reports a path without following a symlink as its target type', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'a', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const info = await fs.lstat('a.txt')
      expect(info?.type).toBe('file')
      expect(await fs.lstat('nope.txt')).toBeUndefined()
    } finally {
      await dispose()
    }
  })

  it('reports a failed call as a typed filesystem error', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      await expect(fs.readText(target)).rejects.toBeInstanceOf(FsError)
    } finally {
      await dispose()
    }
  })

  it('watches for a change', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'a', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      const controller = new AbortController()
      let changed = 0
      const stop = await fs.watch(target, () => { changed += 1 }, controller.signal)
      writeFileSync(join(root, 'a.txt'), 'changed', 'utf8')
      await new Promise(resolve => setTimeout(resolve, 1500))
      await stop()
      expect(changed).toBeGreaterThan(0)
    } finally {
      await dispose()
    }
  })

  it('refuses an aborted watch', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target: FsTarget = { targetKey: FsTargetKey(join(root, 'a.txt')), displayPath: 'a.txt' }
      const controller = new AbortController()
      controller.abort()
      await expect(fs.watch(target, () => {}, controller.signal)).rejects.toThrow()
    } finally {
      await dispose()
    }
  })

  it('refuses to operate once the server is gone', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    await dispose()
    const target = await fs.resolve('a.txt')
    await expect(fs.readText(target)).rejects.toMatchObject({ code: 'FS_IO_ERROR' })
  })

  it('reports a missing file as not found when its bytes are read', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('nope.bin')
      await expect(fs.readBytes(target, undefined, 1024)).rejects.toMatchObject({ code: 'FS_NOT_FOUND' })
    } finally {
      await dispose()
    }
  })

  it('reports a missing file as not found when its text is read', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('nope.txt')
      await expect(fs.readText(target)).rejects.toMatchObject({ code: 'FS_NOT_FOUND' })
    } finally {
      await dispose()
    }
  })

  it('reports a directory as not a directory when its text is read', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('.')
      await expect(fs.readText(target)).rejects.toMatchObject({ code: 'FS_NOT_DIRECTORY' })
    } finally {
      await dispose()
    }
  })

  it('reports a missing file as not found when it is edited', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('nope.txt')
      await expect(fs.editText(target, { oldString: 'a', newString: 'b', replaceAll: false }))
        .rejects.toMatchObject({ code: 'FS_NOT_FOUND' })
    } finally {
      await dispose()
    }
  })

  it('applies the config defaults when a field is omitted', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'defaulted\n', 'utf8')
    const ctx = new Context()
    const fiber = await ctx.plugin(NcToolsFileSystem, {
      command: process.execPath,
      args: [FIXTURE, root],
      workspaceRoot: root,
    })
    try {
      // The omitted fields take the schema's defaults: the argument list, the
      // call budget and the watch interval.
      const target = await ctx.fs.resolve('a.txt')
      expect(await ctx.fs.readText(target)).toBe('defaulted\n')
    } finally {
      await fiber.dispose()
    }
  })

  it('translates a server failure that carries no structure into an io error', async () => {
    const root = workspace()
    writeFileSync(join(root, 'raise-plain.txt'), 'x', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('raise-plain.txt')
      await expect(fs.readText(target)).rejects.toMatchObject({ code: 'FS_IO_ERROR' })
    } finally {
      await dispose()
    }
  })

  it('translates a server failure that carries no code into an io error', async () => {
    const root = workspace()
    writeFileSync(join(root, 'raise-nocode.txt'), 'x', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('raise-nocode.txt')
      await expect(fs.readText(target)).rejects.toMatchObject({ code: 'FS_IO_ERROR' })
    } finally {
      await dispose()
    }
  })

  it('translates a server failure that carries a code into the matching code', async () => {
    const root = workspace()
    writeFileSync(join(root, 'raise-hinted.txt'), 'x', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('raise-hinted.txt')
      await expect(fs.readText(target)).rejects.toMatchObject({ code: 'FS_PERMISSION_DENIED' })
    } finally {
      await dispose()
    }
  })

  it('carries a kind that is neither a file nor a directory through the stat', async () => {
    const root = workspace()
    writeFileSync(join(root, 'odd-type.txt'), 'x', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('odd-type.txt')
      const info = await fs.stat(target)
      expect(info?.type).toBe('other')
    } finally {
      await dispose()
    }
  })

  it('reports a directory without a size when it is inspected', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const info = await fs.lstat('.')
      expect(info?.type).toBe('directory')
      expect(info?.size).toBeUndefined()
    } finally {
      await dispose()
    }
  })

  it('reports a directory as not a directory when its bytes are read', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('.')
      await expect(fs.readBytes(target, undefined, 1024)).rejects.toMatchObject({ code: 'FS_NOT_DIRECTORY' })
    } finally {
      await dispose()
    }
  })

  it('lists a directory entry that is a directory', async () => {
    const root = workspace()
    mkdirSync(join(root, 'sub'))
    writeFileSync(join(root, 'a.txt'), 'a', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('.')
      const entries = await fs.listDir(target)
      const sub = entries.find(entry => entry.name === 'sub')
      expect(sub?.type).toBe('directory')
      expect(sub?.size).toBeUndefined()
    } finally {
      await dispose()
    }
  })

  it('edits against the version it observed', async () => {
    const root = workspace()
    writeFileSync(join(root, 'a.txt'), 'one\n', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('a.txt')
      const info = await fs.stat(target)
      const outcome = await fs.editText(
        target,
        { oldString: 'one', newString: 'two', replaceAll: false },
        { version: info!.version },
      )
      expect(outcome.after).toBe('two\n')
    } finally {
      await dispose()
    }
  })

  it('refuses an edit of a file that was never there', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('nope.txt')
      await expect(fs.editText(
        target,
        { oldString: 'one', newString: 'two', replaceAll: false },
        { version: FsVersion('anything') },
      )).rejects.toMatchObject({ code: 'FS_NOT_FOUND' })
    } finally {
      await dispose()
    }
  })

  it('translates a server failure whose error is not an object', async () => {
    const root = workspace()
    writeFileSync(join(root, 'raise-bare.txt'), 'x', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('raise-bare.txt')
      await expect(fs.readText(target)).rejects.toMatchObject({ code: 'FS_IO_ERROR' })
    } finally {
      await dispose()
    }
  })

  it('translates a server failure whose error carries no message', async () => {
    const root = workspace()
    writeFileSync(join(root, 'raise-nomessage.txt'), 'x', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('raise-nomessage.txt')
      await expect(fs.readText(target)).rejects.toMatchObject({ code: 'FS_IO_ERROR' })
    } finally {
      await dispose()
    }
  })

  it('reads an empty file as empty text and empty bytes', async () => {
    const root = workspace()
    writeFileSync(join(root, 'empty.txt'), '', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('empty.txt')
      expect(await fs.readText(target)).toBe('')
      expect([...(await fs.readBytes(target, undefined, 1024))]).toEqual([])
    } finally {
      await dispose()
    }
  })

  it('watches a directory', async () => {
    const root = workspace()
    mkdirSync(join(root, 'sub'))
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('sub')
      const controller = new AbortController()
      const stop = await fs.watch(target, () => {}, controller.signal)
      await stop()
    } finally {
      await dispose()
    }
  })

  it('reports a directory as not a directory when it is written', async () => {
    const root = workspace()
    mkdirSync(join(root, 'sub'))
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('sub')
      await expect(fs.writeText(target, 'x')).rejects.toMatchObject({ code: 'FS_NOT_DIRECTORY' })
    } finally {
      await dispose()
    }
  })

  it('refuses a replace against a file that was never there', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('nope.txt')
      await expect(fs.writeText(target, 'x', { kind: 'replaceIfVersion', version: FsVersion('anything') }))
        .rejects.toMatchObject({ code: 'FS_STALE_VERSION' })
    } finally {
      await dispose()
    }
  })

  it('refuses to report a version for a file it cannot digest', async () => {
    const root = workspace()
    writeFileSync(join(root, 'no-hash.txt'), 'x', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('no-hash.txt')
      await expect(fs.stat(target)).rejects.toMatchObject({ code: 'FS_NOT_FOUND' })
    } finally {
      await dispose()
    }
  })

  it('creates a file that is not there under a create guard', async () => {
    const root = workspace()
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('fresh.txt')
      const outcome = await fs.writeText(target, 'new\n', { kind: 'createIfAbsent' })
      expect(outcome.operation).toBe('create')
      expect(outcome.before).toBeNull()
    } finally {
      await dispose()
    }
  })

  it('reports a watcher poll that fails', async () => {
    const root = workspace()
    writeFileSync(join(root, 'raise-stat.txt'), 'x', 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('raise-stat.txt')
      const controller = new AbortController()
      const failures: Error[] = []
      const stop = await fs.watch(target, (error) => { if (error !== undefined) failures.push(error) }, controller.signal)
      await new Promise(resolve => setTimeout(resolve, 1200))
      await stop()
      expect(failures.length).toBeGreaterThan(0)
    } finally {
      await dispose()
    }
  })

  it('reads a file larger than one window', async () => {
    const root = workspace()
    // Larger than the server's 1 MiB window, so the read takes more than one
    // round trip and the windows have to be chained.
    const content = 'x'.repeat(1_500_000)
    writeFileSync(join(root, 'big.txt'), content, 'utf8')
    const { fs, dispose } = await setup(root)
    try {
      const target = await fs.resolve('big.txt')
      expect(await fs.readText(target)).toHaveLength(content.length)
    } finally {
      await dispose()
    }
  })
})
