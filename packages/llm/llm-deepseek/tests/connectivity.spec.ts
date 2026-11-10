import { afterEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import type { AnonymousUserId } from '@deepseek-ai/dsh-anonymous-user-id'
import LlmRuntime from '@deepseek-ai/dsh-llm'
import * as Messages from '@deepseek-ai/dsh-llm-deepseek-api-key'
import { testDeepSeekModel } from '../src/connectivity.ts'
import { resolveAdapterOptions } from '../src/config.ts'
import { MODEL, server, sse, textEvents } from './helpers.ts'

const USER_ID = '00000000-0000-4000-8000-000000000001' as AnonymousUserId
/** Namespace the plugin reports for a hand-mounted composition. */
const NS = 'llm-deepseek-api-key'

const closing: (() => Promise<void>)[] = []
afterEach(async () => {
  while (closing.length) await closing.pop()!()
  vi.unstubAllEnvs()
})

/** Loopback Messages endpoint with teardown registered for this spec. */
async function endpoint(...args: Parameters<typeof server>) {
  const instance = await server(...args)
  closing.push(() => instance.close())
  return instance
}

/** The official route, mounted over one loopback Messages endpoint. */
async function harness(baseURL: string): Promise<Context> {
  vi.stubEnv('DEEPSEEK_API_KEY', 'stored-deepseek-key')
  const ctx = new Context()
  await ctx.plugin(LlmRuntime)
  await ctx.plugin(Messages, { baseURL })
  return ctx
}

describe('stored-route model connectivity', () => {
  it('probes a registered route through the seam with its stored credential', async () => {
    const http = await endpoint()
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(NS, { provider: 'deepseek-official', model: MODEL })

    expect(result).toMatchObject({ ok: true, model: MODEL })
    expect(result.failure).toBeUndefined()
    expect(result.latencyMs).toBeGreaterThanOrEqual(0)
    expect(http.requests).toHaveLength(1)
    expect(http.requests[0]).toMatchObject({
      path: '/anthropic/v1/messages',
      headers: { 'x-api-key': 'stored-deepseek-key', 'anthropic-version': '2023-06-01' },
      body: { model: MODEL, stream: true },
    })
    // The probe is one tiny request, not a conversation.
    expect(JSON.stringify(http.requests[0]?.body)).toContain('Connectivity check')
  })

  it('reports the provider refusal with its own status', async () => {
    const http = await endpoint((response) => {
      response.writeHead(400, { 'content-type': 'application/json' })
      response.end(JSON.stringify({ error: { type: 'invalid_request_error', message: 'unknown model' } }))
    })
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(NS, { provider: 'deepseek-official', model: 'not-offered' })

    expect(result).toMatchObject({
      ok: false,
      model: 'not-offered',
      failure: { message: 'unknown model', code: 'INVALID_REQUEST', status: 400 },
    })
  })

  it('reports a refused credential as the answer rather than a call failure', async () => {
    const http = await endpoint((response) => {
      response.writeHead(401, { 'content-type': 'application/json' })
      response.end(JSON.stringify({ error: { type: 'authentication_error', message: 'Authentication Failed' } }))
    })
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(NS, { provider: 'deepseek-official', model: MODEL })

    expect(result).toMatchObject({ ok: false, failure: { code: 'AUTH', status: 401 } })
    expect(result.failure?.message).toContain('Authentication Failed')
    expect(http.requests).toHaveLength(1)
  })

  it('refuses a probe that names no model', async () => {
    const http = await endpoint()
    const ctx = await harness(http.url)

    await expect(ctx.llm.testModel(NS, { provider: 'deepseek-official', model: '' }))
      .rejects.toMatchObject({ code: 'INVALID_TEST' })
    expect(http.requests).toEqual([])
  })

  it('withdraws the offer when the plugin unloads', async () => {
    const http = await endpoint()
    const ctx = new Context()
    vi.stubEnv('DEEPSEEK_API_KEY', 'stored-deepseek-key')
    await ctx.plugin(LlmRuntime)
    const fiber = await ctx.plugin(Messages, { baseURL: http.url })
    await expect(ctx.llm.testModel(NS, { provider: 'deepseek-official', model: MODEL }))
      .resolves.toMatchObject({ ok: true })

    await fiber.dispose()

    await expect(ctx.llm.testModel(NS, { provider: 'deepseek-official', model: MODEL }))
      .rejects.toMatchObject({ code: 'NO_TEST' })
  })

  it('refuses a namespace nothing serves', async () => {
    const http = await endpoint()
    const ctx = await harness(http.url)

    await expect(ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek-official', model: MODEL }))
      .rejects.toMatchObject({ code: 'NO_TEST' })
    expect(http.requests).toEqual([])
  })
})

describe('draft-route model connectivity', () => {
  it('sends a typed key to the endpoint it was typed for', async () => {
    const http = await endpoint()
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(NS, { baseURL: http.url, apiKey: 'typed-key', model: MODEL })

    expect(result).toMatchObject({ ok: true })
    expect(http.requests[0]).toMatchObject({ headers: { 'x-api-key': 'typed-key' } })
  })

  it('tests a key against the edited route without resolving the stored one', async () => {
    const http = await endpoint()
    const ctx = new Context()
    // The stored reference resolves to nothing here: a probe that tried to
    // resolve it would fail over a key the user is not asking about.
    vi.stubEnv('DEEPSEEK_API_KEY', '')
    await ctx.plugin(LlmRuntime)
    await ctx.plugin(Messages, { baseURL: http.url })

    const result = await ctx.llm.testModel(NS, { provider: 'deepseek-official', model: MODEL, apiKey: 'typed-key' })

    expect(result).toMatchObject({ ok: true })
    expect(http.requests[0]).toMatchObject({
      path: '/anthropic/v1/messages',
      headers: { 'x-api-key': 'typed-key' },
    })
  })

  it('authenticates a paused route from its own settings when the draft names no key', async () => {
    const http = await endpoint()
    const ctx = new Context()
    vi.stubEnv('DEEPSEEK_API_KEY', 'stored-deepseek-key')
    await ctx.plugin(LlmRuntime)
    // `enabled: false` withdraws the route from the registry, so the probe has
    // no seam entry and has to build the connection itself.
    await ctx.plugin(Messages, { baseURL: http.url, enabled: false })

    const result = await ctx.llm.testModel(NS, { provider: 'deepseek-official', model: MODEL })

    expect(result).toMatchObject({ ok: true })
    expect(http.requests[0]).toMatchObject({ headers: { 'x-api-key': 'stored-deepseek-key' } })
  })

  it('probes an endpoint with no credential when no route is named', async () => {
    const http = await endpoint((response) => {
      response.writeHead(401, { 'content-type': 'application/json' })
      response.end(JSON.stringify({ error: { message: 'no credential' } }))
    })
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(NS, { baseURL: http.url, model: MODEL })

    expect(result).toMatchObject({ ok: false, failure: { code: 'AUTH', status: 401 } })
    expect(http.requests[0]?.headers['x-api-key']).toBeUndefined()
  })

  it('probes a draft endpoint with a model the section does not declare', async () => {
    const http = await endpoint()
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(NS, { baseURL: http.url, apiKey: 'k', model: 'brand-new-model' })

    expect(result).toMatchObject({ ok: true, model: 'brand-new-model' })
    expect(http.requests[0]).toMatchObject({ body: { model: 'brand-new-model' } })
  })

  it('refuses a protocol the Messages family cannot speak', async () => {
    const http = await endpoint()
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(NS, { baseURL: http.url, api: 'openai-completions', apiKey: 'k', model: MODEL })

    expect(result).toMatchObject({ ok: false, failure: { code: 'UNSUPPORTED_OPTION' } })
    expect(http.requests).toEqual([])
  })

  it('reports an unusable probe key without reaching the network', async () => {
    const http = await endpoint()
    const ctx = await harness(http.url)

    const blank = await ctx.llm.testModel(NS, { baseURL: http.url, apiKey: '', model: MODEL })
    const illegal = await ctx.llm.testModel(NS, { baseURL: http.url, apiKey: 'sk-\u{1F600}', model: MODEL })

    expect(blank).toMatchObject({ ok: false, failure: { code: 'INVALID_CREDENTIAL' } })
    expect(blank.failure?.message).toMatch(/API key is blank/)
    expect(illegal).toMatchObject({ ok: false, failure: { code: 'INVALID_CREDENTIAL' } })
    expect(illegal.failure?.message).toMatch(/no HTTP header can carry/)
    expect(http.requests).toEqual([])
  })

  it('refuses a request with neither route nor endpoint', async () => {
    // The seam rejects that shape outright, so this family's own guard for it is
    // reachable only through the module directly.
    const ctx = new Context()
    await ctx.plugin(LlmRuntime)

    const result = await testDeepSeekModel({ model: MODEL }, {
      seam: ctx.llm,
      options: () => resolveAdapterOptions({}),
      routeAuth: () => Promise.resolve({ headers: {} }),
      resolveUserId: () => USER_ID,
    })

    expect(result).toMatchObject({ ok: false, failure: { code: 'INVALID_TEST' } })
  })
})

describe('model connectivity cancellation', () => {
  it('reports a caller that cancels before dispatch as cancellation', async () => {
    const http = await endpoint()
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(
      NS,
      { provider: 'deepseek-official', model: MODEL },
      AbortSignal.abort('closed the dialog'),
    )

    expect(result).toMatchObject({ ok: false, failure: { code: 'ABORTED', message: 'model test aborted by caller' } })
    expect(http.requests).toEqual([])
  })

  it('reports a caller that cancels mid-response as cancellation, not a provider rejection', async () => {
    const started = Promise.withResolvers<void>()
    const http = await endpoint((response) => {
      response.setHeader('content-type', 'text/event-stream')
      response.flushHeaders()
      // One opening frame, then silence: the abort lands on a live stream.
      response.write(sse([textEvents[0]]))
      started.resolve()
    })
    const ctx = await harness(http.url)
    const controller = new AbortController()

    const pending = ctx.llm.testModel(NS, { provider: 'deepseek-official', model: MODEL }, controller.signal)
    await started.promise
    controller.abort('closed the dialog')

    await expect(pending).resolves.toMatchObject({
      ok: false,
      failure: { code: 'ABORTED', message: 'model test aborted by caller' },
    })
    expect(http.requests).toHaveLength(1)
  })

  it('cancels the provider request once the probe has answered', async () => {
    const disconnected = Promise.withResolvers<void>()
    const http = await endpoint((response) => {
      response.setHeader('content-type', 'text/event-stream')
      response.flushHeaders()
      response.write(sse(textEvents))
      // Left open: only the probe abandoning the response ends the exchange.
      response.on('close', () => { disconnected.resolve() })
    })
    const ctx = await harness(http.url)

    const result = await ctx.llm.testModel(NS, { provider: 'deepseek-official', model: MODEL })

    expect(result).toMatchObject({ ok: true })
    await expect(disconnected.promise).resolves.toBeUndefined()
  })
})
