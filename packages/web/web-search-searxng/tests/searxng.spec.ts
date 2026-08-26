import { afterEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import WebRuntime from '@deepseek-ai/dsh-web'
import { SearxngSearchProvider, SEARXNG_PROVIDER_ID } from '@deepseek-ai/dsh-web-search-searxng'
import * as searxngPlugin from '@deepseek-ai/dsh-web-search-searxng'
import { mapSearxngResponse, mapSearxngResult } from '../src/provider.ts'

const options = { baseURL: 'https://searx.test', username: '', password: '' }

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' }, ...init })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('SearXNG result mapping', () => {
  it('maps a full result entry', () => {
    expect(mapSearxngResult({
      url: 'https://a.test',
      title: 'A',
      content: 'text',
      publishedDate: '2026-01-01',
      engine: 'wikipedia',
    })).toEqual({ url: 'https://a.test', title: 'A', snippet: 'text', publishedAt: '2026-01-01' })
  })

  it('skips a result without a usable URL', () => {
    expect(mapSearxngResult({})).toBeUndefined()
    expect(mapSearxngResult({ url: null })).toBeUndefined()
    expect(mapSearxngResult({ url: '   ' })).toBeUndefined()
  })

  it('omits null/empty optional fields rather than emitting them', () => {
    expect(mapSearxngResult({ url: 'https://a.test', title: null, content: null, publishedDate: null }))
      .toEqual({ url: 'https://a.test' })
    expect(mapSearxngResult({ url: 'https://a.test', title: '', content: '', publishedDate: '' }))
      .toEqual({ url: 'https://a.test' })
  })

  it('joins non-blank answers into content and drops blank ones', () => {
    expect(mapSearxngResponse({ results: [], answers: ['first', '  ', 'second'] }))
      .toEqual({ content: 'first\n\nsecond', sources: [], truncated: false })
  })

  it('omits content when answers are absent or all blank', () => {
    const result = mapSearxngResponse({ results: [], answers: ['  '] })
    expect(result.content).toBeUndefined()
    expect(mapSearxngResponse({}).content).toBeUndefined()
  })

  it('maps a response to filtered sources and no content', () => {
    const result = mapSearxngResponse({
      results: [
        { url: 'https://a.test', content: 'one' },
        { title: 'no url here' },
        { url: 'https://c.test', title: 'C' },
      ],
    })
    expect(result).toEqual({
      sources: [
        { url: 'https://a.test', snippet: 'one' },
        { url: 'https://c.test', title: 'C' },
      ],
      truncated: false,
    })
    expect(result.content).toBeUndefined()
  })

  it('tolerates missing arrays', () => {
    const result = mapSearxngResponse({})
    expect(result.sources).toEqual([])
    expect(result.content).toBeUndefined()
  })

})

describe('SearxngSearchProvider availability', () => {
  it('is available with a parseable base URL', () => {
    expect(new SearxngSearchProvider(options).available()).toBe(true)
  })

  it('accepts the full basic-auth pair', () => {
    expect(new SearxngSearchProvider({ ...options, username: 'alice', password: 's3cret' }).available()).toBe(true)
  })

  it('is unavailable without a base URL', () => {
    expect(new SearxngSearchProvider({ ...options, baseURL: '' }).available()).toBe(false)
  })

  it('is unavailable when the base URL is unparseable', () => {
    expect(new SearxngSearchProvider({ ...options, baseURL: 'not a url' }).available()).toBe(false)
  })

  it('is unavailable with a half-configured basic-auth pair', () => {
    expect(new SearxngSearchProvider({ ...options, username: 'alice' }).available()).toBe(false)
    expect(new SearxngSearchProvider({ ...options, password: 's3cret' }).available()).toBe(false)
  })
})

describe('SearxngSearchProvider request mapping', () => {
  it('sends the encoded query as a JSON-format GET request', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ results: [] }))
    vi.stubGlobal('fetch', fetchMock)

    await new SearxngSearchProvider(options).search({ query: 'hello world', maxResults: 5 })

    expect(fetchMock).toHaveBeenCalledOnce()
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('https://searx.test/search?q=hello%20world&format=json')
    expect(init).toMatchObject({ redirect: 'error' })
    const headers = init.headers as Record<string, string>
    expect(headers['accept']).toBe('application/json')
    expect(headers['user-agent']).toBe('deepseek-harness/0.0.1')
    expect(headers.authorization).toBeUndefined()
  })

  it('appends no count control: maxResults is enforced by the seam alone', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ results: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await new SearxngSearchProvider(options).search({ query: 'q', maxResults: 3 })
    const [url] = fetchMock.mock.calls[0] as unknown as [string]
    expect(url).toBe('https://searx.test/search?q=q&format=json')
  })

  it('sends basic auth only when both credentials are configured', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ results: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await new SearxngSearchProvider(options).search({ query: 'q' })
    let [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect((init.headers as Record<string, string>).authorization).toBeUndefined()

    await new SearxngSearchProvider({ ...options, username: 'alice', password: 's3cret' }).search({ query: 'q' })
    ;[, init] = fetchMock.mock.calls[1] as unknown as [string, RequestInit]
    expect((init.headers as Record<string, string>).authorization).toBe(`Basic ${btoa('alice:s3cret')}`)
  })

  it('forwards the abort signal', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ results: [] }))
    vi.stubGlobal('fetch', fetchMock)
    const controller = new AbortController()
    await new SearxngSearchProvider(options).search({ query: 'q' }, controller.signal)
    const [, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(init.signal).toBe(controller.signal)
  })
})

describe('SearxngSearchProvider error handling', () => {
  it('maps an HTTP error to WEB_PROVIDER_ERROR with the provider message', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ error: 'Invalid format' }, { status: 403 })))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_ERROR', message: 'Invalid format' }))
  })

  it('falls back to the message field when the error body carries no error field', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ message: 'rate limited' }, { status: 429 })))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_ERROR', message: 'rate limited' }))
  })

  it('quotes the first line of a non-JSON error body', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('gateway down', { status: 502 })))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_ERROR', message: 'gateway down' }))
  })

  it('keeps the status-line message when the JSON error body carries no detail', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({}, { status: 500 })))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ message: 'SearXNG API error (HTTP 500)' }))
  })

  it('keeps the status-line message when the JSON error detail is empty', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ error: '', message: '' }, { status: 500 })))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ message: 'SearXNG API error (HTTP 500)' }))
  })

  it('maps a network failure to WEB_PROVIDER_ERROR', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new TypeError('connection refused'))))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_ERROR' }))
  })

  it('maps an abort to WEB_ABORTED', async () => {
    vi.stubGlobal('fetch', vi.fn(() => Promise.reject(new DOMException('aborted', 'AbortError'))))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_ABORTED' }))
  })

  it('maps an unparseable success body to WEB_PROVIDER_ERROR', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('not json', { status: 200 })))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_ERROR' }))
  })

  it('maps a well-formed body of the wrong shape to WEB_PROVIDER_ERROR, not a raw TypeError', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ results: {} }, { status: 200 })))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_ERROR' }))
  })

  it('surfaces an abort during success-body parse as WEB_ABORTED, not provider error', async () => {
    const body = { json: () => Promise.reject(new DOMException('aborted', 'AbortError')), ok: true, status: 200 }
    vi.stubGlobal('fetch', vi.fn(async () => body as unknown as Response))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_ABORTED' }))
  })

  it('surfaces an abort during error-body read as WEB_ABORTED', async () => {
    // The shared error-body reader consumes the stream, so the fake aborts
    // by erroring its body stream rather than rejecting a json() call.
    const body = {
      ok: false,
      status: 500,
      body: new ReadableStream<Uint8Array>({
        start(controller) { controller.error(new DOMException('aborted', 'AbortError')) },
      }),
    }
    vi.stubGlobal('fetch', vi.fn(async () => body as unknown as Response))
    await expect(new SearxngSearchProvider(options).search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_ABORTED' }))
  })
})

describe('web-search-searxng plugin registration', () => {
  it('registers the provider into ctx.web (HMR-safe)', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse({ results: [] })))
    const ctx = new Context()
    await ctx.plugin(WebRuntime, { searchProvider: SEARXNG_PROVIDER_ID })
    const fiber = await ctx.plugin(searxngPlugin, { baseURL: 'https://searx.test' })
    await expect(ctx.web.search({ query: 'q' })).resolves.toMatchObject({ sources: [], truncated: false })
    await fiber.dispose()
    await expect(ctx.web.search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_CONFIGURED_MISSING' }))
  })

  it('has no default export (namespace plugin export shape)', () => {
    expect('default' in searxngPlugin).toBe(false)
  })

  it('threads baseURL, username and password config into the request', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({
      results: [{ url: 'https://a.test', title: 'A', content: 'text' }],
      answers: ['instant answer'],
    }))
    vi.stubGlobal('fetch', fetchMock)
    const ctx = new Context()
    await ctx.plugin(WebRuntime, { searchProvider: SEARXNG_PROVIDER_ID })
    const fiber = await ctx.plugin(searxngPlugin, { baseURL: 'https://threaded.test', username: 'alice', password: 's3cret' })
    await expect(ctx.web.search({ query: 'q', maxResults: 10 })).resolves.toMatchObject({
      content: 'instant answer',
      sources: [{ url: 'https://a.test', title: 'A', snippet: 'text' }],
    })
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe('https://threaded.test/search?q=q&format=json')
    expect((init.headers as Record<string, string>).authorization).toBe(`Basic ${btoa('alice:s3cret')}`)
    await fiber.dispose()
  })

  it('falls back to $SEARXNG_BASE_URL when config omits baseURL', async () => {
    const prev = process.env.SEARXNG_BASE_URL
    process.env.SEARXNG_BASE_URL = 'https://env-searx.test'
    try {
      const fetchMock = vi.fn(async () => jsonResponse({ results: [] }))
      vi.stubGlobal('fetch', fetchMock)
      const ctx = new Context()
      await ctx.plugin(WebRuntime, { searchProvider: SEARXNG_PROVIDER_ID })
      const fiber = await ctx.plugin(searxngPlugin, {})
      await ctx.web.search({ query: 'q' })
      const [url] = fetchMock.mock.calls[0] as unknown as [string]
      expect(url).toBe('https://env-searx.test/search?q=q&format=json')
      await fiber.dispose()
    } finally {
      if (prev === undefined) delete process.env.SEARXNG_BASE_URL
      else process.env.SEARXNG_BASE_URL = prev
    }
  })

  it('is unavailable when neither config nor env supplies a base URL', async () => {
    const prev = process.env.SEARXNG_BASE_URL
    delete process.env.SEARXNG_BASE_URL
    try {
      const ctx = new Context()
      await ctx.plugin(WebRuntime, { searchProvider: SEARXNG_PROVIDER_ID })
      await ctx.plugin(searxngPlugin, {})
      await expect(ctx.web.search({ query: 'q' }))
        .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_CONFIGURED_UNAVAILABLE' }))
    } finally {
      if (prev !== undefined) process.env.SEARXNG_BASE_URL = prev
    }
  })

  it('is unavailable when config supplies only half of the basic-auth pair', async () => {
    const ctx = new Context()
    await ctx.plugin(WebRuntime, { searchProvider: SEARXNG_PROVIDER_ID })
    await ctx.plugin(searxngPlugin, { baseURL: 'https://searx.test', username: 'alice' })
    await expect(ctx.web.search({ query: 'q' }))
      .rejects.toThrow(expect.objectContaining({ code: 'WEB_PROVIDER_CONFIGURED_UNAVAILABLE' }))
  })

  it('rejects non-Latin-1 credentials at load instead of failing per search', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ results: [] }))
    vi.stubGlobal('fetch', fetchMock)
    const ctx = new Context()
    await ctx.plugin(WebRuntime, { searchProvider: SEARXNG_PROVIDER_ID })
    // `btoa` cannot encode code points above U+00FF; the misconfiguration is
    // known at load, so apply() must reject before any request can fail.
    await expect(ctx.plugin(searxngPlugin, { baseURL: 'https://searx.test', username: 'ali', password: 'passΩword' }))
      .rejects.toThrow(/web-search-searxng: username and password must be Latin-1 encodable/)
    expect(fetchMock).not.toHaveBeenCalled()
  })
})
