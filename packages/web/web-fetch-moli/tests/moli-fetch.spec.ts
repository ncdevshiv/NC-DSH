import { describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import WebRuntime, { WebError } from '@deepseek-ai/dsh-web'
import {
  MOLI_FETCH_PROVIDER_ID,
  MoliFetchProvider,
  validateFetchUrl,
} from '@deepseek-ai/dsh-web-fetch-moli'
import type { NativeCommandRunner } from '@deepseek-ai/dsh-native-command'
import * as moliPlugin from '@deepseek-ai/dsh-web-fetch-moli'

/** A successful runner result. */
function ok(stdout: string): { stdout: string; stderr: string } {
  return { stdout, stderr: '' }
}

/** A runner that always resolves with the given stdout. */
function resolvingRunner(stdout: string): NativeCommandRunner {
  return () => Promise.resolve(ok(stdout))
}

/** A provider wired to injected fakes so no test touches a real process. */
function makeProvider(overrides: Partial<ConstructorParameters<typeof MoliFetchProvider>[0]> = {}): MoliFetchProvider {
  const prober = overrides.prober ?? (() => ({ status: 0, error: null }))
  return new MoliFetchProvider({
    binaryPath: 'moli-fake',
    maxBodyChars: 100_000,
    timeoutMs: 30_000,
    probeTimeoutMs: 5_000,
    runner: resolvingRunner('# hello'),
    prober,
    ...overrides,
  })
}

describe('MoliFetchProvider retrieval', () => {
  it('returns rendered markdown as kind:text with statusCode 200', async () => {
    const provider = makeProvider({ runner: resolvingRunner('# Title\n\nBody') })
    await expect(provider.fetch({ url: 'https://example.com' }))
      .resolves.toEqual({
        url: 'https://example.com',
        statusCode: 200,
        body: { kind: 'text', content: '# Title\n\nBody' },
        truncated: false,
      })
  })

  it('passes the dump argv to the runner', async () => {
    const runner = vi.fn(resolvingRunner('content'))
    const provider = makeProvider({ runner, binaryPath: 'custom-moli' })
    await provider.fetch({ url: 'https://example.com/x' })
    expect(runner).toHaveBeenCalledWith('custom-moli', ['fetch', '--dump', 'markdown', '--wait-until', 'done', 'https://example.com/x'], expect.any(AbortSignal))
  })

  it('truncates beyond maxBodyChars and flags it', async () => {
    const provider = makeProvider({ runner: resolvingRunner('abcdef'), maxBodyChars: 3 })
    const result = await provider.fetch({ url: 'https://example.com' })
    expect(result.body.content).toBe('abc')
    expect(result.truncated).toBe(true)
  })

  it('keeps an exact-boundary body untruncated', async () => {
    const provider = makeProvider({ runner: resolvingRunner('abcdef'), maxBodyChars: 6 })
    const result = await provider.fetch({ url: 'https://example.com' })
    expect(result.body.content).toBe('abcdef')
    expect(result.truncated).toBe(false)
  })

  it('rejects empty stdout as a provider error', async () => {
    const provider = makeProvider({ runner: resolvingRunner('') })
    await expect(provider.fetch({ url: 'https://example.com' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_ERROR', message: 'moli produced no content' }))
  })
})

describe('MoliFetchProvider failure classification', () => {
  it('classifies ENOENT as a missing-binary provider error naming the fix', async () => {
    const failure = Object.assign(new Error('spawn moli-fake ENOENT'), { code: 'ENOENT' })
    const provider = makeProvider({ runner: () => Promise.reject(failure), binaryPath: 'moli-fake' })
    const error = await provider.fetch({ url: 'https://example.com' }).then(() => undefined, (e: unknown) => e)
    expect((error as WebError).code).toBe('WEB_PROVIDER_ERROR')
    expect((error as WebError).message).toContain('the moli binary was not found at "moli-fake"')
  })

  it('quotes at most the stderr tail on a non-zero exit', async () => {
    const failure = Object.assign(new Error('exit 1'), { code: 1, stderr: `${'x'.repeat(600)}TAIL` })
    const provider = makeProvider({ runner: () => Promise.reject(failure) })
    const error = await provider.fetch({ url: 'https://example.com' }).then(() => undefined, (e: unknown) => e)
    expect((error as WebError).code).toBe('WEB_PROVIDER_ERROR')
    expect((error as WebError).message).toMatch(/^moli fetch failed: x+TAIL$/)
  })

  it('falls back to the error message when stderr is empty', async () => {
    const provider = makeProvider({ runner: () => Promise.reject(new Error('boom')) })
    await expect(provider.fetch({ url: 'https://example.com' }))
      .rejects.toThrow(expect.objectContaining({ message: 'moli fetch failed: Error: boom' }))
  })

  it('classifies an AbortError rejection as WEB_ABORTED', async () => {
    const provider = makeProvider({ runner: () => Promise.reject(new DOMException('aborted', 'AbortError')) })
    await expect(provider.fetch({ url: 'https://example.com' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_ABORTED' }))
  })

  it('classifies an already-aborted caller signal as WEB_ABORTED even on generic failures', async () => {
    const controller = new AbortController()
    controller.abort()
    const provider = makeProvider({ runner: () => Promise.reject(new Error('killed early')) })
    await expect(provider.fetch({ url: 'https://example.com' }, controller.signal))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_ABORTED' }))
  })

  it('reports WEB_FETCH_TIMEOUT when the backstop fires first', async () => {
    const hangsUntilAborted: NativeCommandRunner = (_command, _args, signal) =>
      new Promise((_resolve, reject) => {
        signal.addEventListener('abort', () => { reject(new Error('terminated')) }, { once: true })
      })
    const provider = makeProvider({ runner: hangsUntilAborted, timeoutMs: 10 })
    await expect(provider.fetch({ url: 'https://example.com' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_FETCH_TIMEOUT' }))
  })

  it('reports WEB_FETCH_TIMEOUT when the killed run rejects as an AbortError', async () => {
    // Ordering guard: a transport may surface a kill as a DOMException rather
    // than a plain Error; once the backstop has fired, the failure is a
    // timeout even though the thrown value looks like an abort.
    const rejectsAbortAfterBackstop: NativeCommandRunner = (_command, _args, signal) =>
      new Promise((_resolve, reject) => {
        signal.addEventListener('abort', () => {
          setTimeout(() => { reject(new DOMException('aborted', 'AbortError')) }, 5)
        }, { once: true })
      })
    const provider = makeProvider({ runner: rejectsAbortAfterBackstop, timeoutMs: 10 })
    await expect(provider.fetch({ url: 'https://example.com' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_FETCH_TIMEOUT' }))
  })

  it('never spawns the subprocess when the caller signal is already aborted', async () => {
    const controller = new AbortController()
    controller.abort()
    const runner = vi.fn(resolvingRunner('content'))
    const provider = makeProvider({ runner })
    await expect(provider.fetch({ url: 'https://example.com' }, controller.signal))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_ABORTED' }))
    expect(runner).not.toHaveBeenCalled()
  })
})

describe('MoliFetchProvider availability', () => {
  it('probes once and memoizes the result', () => {
    const prober = vi.fn(() => ({ status: 0, error: null }))
    const provider = makeProvider({ prober })
    expect(provider.available()).toBe(true)
    expect(provider.available()).toBe(true)
    expect(prober).toHaveBeenCalledTimes(1)
    expect(prober).toHaveBeenCalledWith('moli-fake', 5_000)
  })

  it('is unavailable when the probe fails', () => {
    const provider = makeProvider({ prober: () => ({ status: null, error: new Error('ENOENT') }) })
    expect(provider.available()).toBe(false)
  })
})

describe('moli fetch URL policy', () => {
  it.each([
    ['ftp://example.com/x', 'WEB_INVALID_URL'],
    ['not a url', 'WEB_INVALID_URL'],
    ['https://user:pass@example.com/', 'WEB_BLOCKED_URL'],
  ])('rejects %s with %s', (input, code) => {
    expect(() => validateFetchUrl(input)).toThrow(expect.objectContaining({ code }))
  })

  it('rejects over-long URLs before parsing', () => {
    expect(() => validateFetchUrl(`https://example.com/${'a'.repeat(2048)}`))
      .toThrow(expect.objectContaining({ code: 'WEB_INVALID_URL' }))
  })

  it('accepts a plain https URL and returns it parsed', () => {
    expect(validateFetchUrl('https://example.com/').hostname).toBe('example.com')
  })
})

describe('web-fetch-moli plugin registration', () => {
  it('registers the provider into ctx.web and disposes with the fiber (HMR-safe)', async () => {
    const ctx = new Context()
    await ctx.plugin(WebRuntime, { fetchProvider: MOLI_FETCH_PROVIDER_ID })
    // A nonexistent binary name keeps the registered provider unavailable
    // without launching anything: the probe fails fast at path lookup.
    const fiber = await ctx.plugin(moliPlugin, { binaryPath: 'dsh-test-no-such-moli-binary' })
    await expect(ctx.web.fetch({ url: 'https://example.com/' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_CONFIGURED_UNAVAILABLE' }))
    await fiber.dispose()
    await expect(ctx.web.fetch({ url: 'https://example.com/' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_CONFIGURED_MISSING' }))
  })

  it('has no default export (namespace plugin export shape)', () => {
    expect('default' in moliPlugin).toBe(false)
  })

  it('rejects non-positive limits at construction', async () => {
    const ctx = new Context()
    await ctx.plugin(WebRuntime)
    await expect(ctx.plugin(moliPlugin, { maxBodyChars: 0 }))
      .rejects.toThrow(/maxBodyChars must be a positive finite number/)
    await expect(ctx.plugin(moliPlugin, { timeoutMs: -1 }))
      .rejects.toThrow(/timeoutMs must be a positive finite number/)
    await expect(ctx.plugin(moliPlugin, { probeTimeoutMs: 0 }))
      .rejects.toThrow(/probeTimeoutMs must be a positive finite number/)
  })

  it('rejects a timeout beyond Node timer range at construction', async () => {
    const ctx = new Context()
    await ctx.plugin(WebRuntime)
    await expect(ctx.plugin(moliPlugin, { timeoutMs: 2_147_483_648 }))
      .rejects.toThrow(/timeoutMs must be no greater than 2147483647/)
  })
})
