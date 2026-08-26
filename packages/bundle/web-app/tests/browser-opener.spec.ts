/** Default-browser opener selection, platform argv, env scrubbing, and error mapping. */

import { EventEmitter } from 'node:events'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  buildPlatformOpenerCommand,
  openDefaultBrowser,
  openWithChildProcess,
  probeBunOpen,
} from '../src/browser-opener.ts'

/** Minimal spawn seam: records the call and hands back a controllable fake child. */
interface RecordedCall {
  command: string
  args: readonly string[]
  options: Record<string, unknown>
}

function makeSpawnStub(behavior: (call: RecordedCall) => { exitCode: number | null; stderr?: string; error?: Error }) {
  const calls: RecordedCall[] = []
  const spawnFn = vi.fn((command: string, args: readonly string[], options: Record<string, unknown>) => {
    const call = { command, args, options }
    calls.push(call)
    const child = Object.assign(new EventEmitter(), {
      stderr: new EventEmitter(),
      stdout: new EventEmitter(),
      stdin: null,
      pid: 42_000,
    }) as unknown as import('node:child_process').ChildProcess
    const stderrStream = child.stderr as unknown as EventEmitter & { setEncoding: () => void }
    stderrStream.setEncoding = () => {}
    queueMicrotask(() => {
      const outcome = behavior(call)
      if (outcome.stderr !== undefined) stderrStream.emit('data', outcome.stderr)
      if (outcome.error !== undefined) {
        child.emit('error', outcome.error)
        return
      }
      // Real children emit `exit` (process gone) before `close` (pipes drained).
      child.emit('exit', outcome.exitCode)
      child.emit('close', outcome.exitCode)
    })
    return child
  }) as unknown as typeof import('node:child_process').spawn
  return { spawnFn, calls }
}

afterEach(() => {
  Reflect.deleteProperty(globalThis, 'Bun')
  vi.restoreAllMocks()
})

describe('probeBunOpen', () => {
  it('is false on a Node runtime without a Bun global', () => {
    expect(probeBunOpen()).toBe(false)
  })

  it('is true when the runtime exposes a function-valued Bun.open', () => {
    ;(globalThis as { Bun?: object }).Bun = { open: () => Promise.resolve() }
    expect(probeBunOpen()).toBe(true)
  })

  it('is false when Bun exists but open is not callable', () => {
    ;(globalThis as { Bun?: object }).Bun = {}
    expect(probeBunOpen()).toBe(false)
  })
})

describe('buildPlatformOpenerCommand', () => {
  it('delegates Windows to cmd /c start with the window-title placeholder', () => {
    const plan = buildPlatformOpenerCommand('win32', 'http://127.0.0.1:4173')
    expect(plan.command).toBe('cmd')
    expect(plan.args).toEqual(['/c', 'start', '', 'http://127.0.0.1:4173'])
    expect(plan.options.windowsHide).toBe(true)
  })

  it('uses /usr/bin/open on macOS and xdg-open on Linux and FreeBSD', () => {
    expect(buildPlatformOpenerCommand('darwin', 'http://x').command).toBe('/usr/bin/open')
    expect(buildPlatformOpenerCommand('linux', 'http://x').command).toBe('xdg-open')
    expect(buildPlatformOpenerCommand('freebsd', 'http://x').command).toBe('xdg-open')
  })

  it('throws for platforms without an opener instead of guessing', () => {
    expect(() => buildPlatformOpenerCommand('android', 'http://x')).toThrow(/opener/)
  })
})

describe('openWithChildProcess', () => {
  it('spawns the platform opener with the scrubbed parent environment', async () => {
    process.env.DEEPSEEK_API_KEY = 'unit-test-secret'
    process.env.DSH_HOME = '/unit/test/home'
    try {
      const { spawnFn, calls } = makeSpawnStub(() => ({ exitCode: 0 }))
      await openWithChildProcess('http://127.0.0.1:4173', 'linux', spawnFn)
      expect(calls).toHaveLength(1)
      const call = calls[0]!
      expect(call.command).toBe('xdg-open')
      const env = call.options.env as Record<string, string | undefined>
      expect(env.DEEPSEEK_API_KEY).toBeUndefined()
      expect(env.DSH_HOME).toBeUndefined()
      expect(env.PATH).toBeDefined()
    } finally {
      delete process.env.DEEPSEEK_API_KEY
      delete process.env.DSH_HOME
    }
  })

  it('resolves when the opener exits zero and forwards its stderr chatter', async () => {
    const writeSpy = vi.spyOn(process.stderr, 'write').mockReturnValue(true)
    const { spawnFn } = makeSpawnStub(() => ({ exitCode: 0, stderr: 'xdg-open: minor warning\n' }))
    await expect(openWithChildProcess('http://x', 'linux', spawnFn)).resolves.toBeUndefined()
    expect(writeSpy).toHaveBeenCalledWith('xdg-open: minor warning\n')
  })

  it('rejects with the first stderr line, Error-prefix stripped, on nonzero exit', async () => {
    const { spawnFn } = makeSpawnStub(() => ({
      exitCode: 1,
      stderr: 'Error: FixtureError: fixture desktop unavailable\r\nsecond line\r\n',
    }))
    await expect(openWithChildProcess('http://x', 'linux', spawnFn)).rejects.toThrow('fixture desktop unavailable')
  })

  it('rejects with the exit code when the failing opener wrote nothing to stderr', async () => {
    const { spawnFn } = makeSpawnStub(() => ({ exitCode: 3 }))
    await expect(openWithChildProcess('http://x', 'linux', spawnFn)).rejects.toThrow('exited with code 3')
  })

  it('rejects with the spawn error itself when the binary cannot be executed', async () => {
    const { spawnFn } = makeSpawnStub(() => ({ exitCode: null, error: new Error('spawn xdg-open ENOENT') }))
    await expect(openWithChildProcess('http://x', 'linux', spawnFn)).rejects.toThrow('ENOENT')
  })

  it('rejects without spawning on a platform that has no opener', async () => {
    const { spawnFn, calls } = makeSpawnStub(() => ({ exitCode: 0 }))
    await expect(openWithChildProcess('http://x', 'android', spawnFn)).rejects.toThrow(/opener/)
    expect(calls).toHaveLength(0)
  })
})

describe('openDefaultBrowser', () => {
  it('prefers the in-process Bun.open when the runtime provides it', async () => {
    const bunOpen = vi.fn(() => Promise.resolve())
    ;(globalThis as { Bun?: object }).Bun = { open: bunOpen }
    const { spawnFn } = makeSpawnStub(() => ({ exitCode: 0 }))
    await openDefaultBrowser('http://bun-path')
    expect(bunOpen).toHaveBeenCalledWith('http://bun-path')
    expect(spawnFn).not.toHaveBeenCalled()
  })

  it('falls back to the child-process opener on a plain Node runtime', async () => {
    const { spawnFn, calls } = makeSpawnStub(() => ({ exitCode: 0 }))
    await openWithChildProcess('http://node-fallback', 'win32', spawnFn)
    expect(calls[0]?.args[0]).toBe('/c')
  })

  it('propagates Bun.open rejections instead of swallowing them', async () => {
    ;(globalThis as { Bun?: object }).Bun = {
      open: () => Promise.reject(new Error('no desktop session')),
    }
    await expect(openDefaultBrowser('http://x')).rejects.toThrow('no desktop session')
  })
})
