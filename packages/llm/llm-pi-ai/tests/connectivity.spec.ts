import { afterEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import LlmRuntime, { userAgent } from '@deepseek-ai/dsh-llm'
import * as LlmPiAi from '@deepseek-ai/dsh-llm-pi-ai'
import type { LlmModelTestRequest } from '@deepseek-ai/dsh-llm'
import { anthropicTextEvents, closeMockServers, mockServer, textEvents } from './mock-server.ts'

/** Credential variables a test set, cleared so the next one starts unset. */
const touchedEnv: string[] = []

afterEach(async () => {
  for (const name of touchedEnv.splice(0)) Reflect.deleteProperty(process.env, name)
  await closeMockServers()
})

function storedKey(name: string, value: string): void {
  process.env[name] = value
  touchedEnv.push(name)
}

/** One configured route served by the mock endpoint, with a deployment header. */
async function harness(baseURL: string): Promise<Context> {
  const ctx = new Context()
  storedKey('PI_CONNECT_KEY', 'stored-key')
  await ctx.plugin(LlmRuntime)
  await ctx.plugin(LlmPiAi, {
    providers: {
      deepseek: { apiKeyEnv: 'PI_CONNECT_KEY', baseURL, headers: { 'X-Tenant': 'private-tenant' } },
    },
  })
  return ctx
}

describe('stored-route model connectivity', () => {
  it('probes a registered route through the seam with its stored credential and headers', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek', model: 'deepseek-flash' })

    expect(result).toMatchObject({ ok: true, model: 'deepseek-flash' })
    expect(result.latencyMs).toBeGreaterThanOrEqual(0)
    expect(result.failure).toBeUndefined()
    expect(server.paths).toEqual(['/chat/completions'])
    expect(server.headers[0]?.authorization).toBe('Bearer stored-key')
    expect(server.headers[0]?.['x-tenant']).toBe('private-tenant')
    expect(server.headers[0]?.['user-agent']).toBe(userAgent())
    // The probe is one tiny request, not a conversation.
    expect(JSON.stringify(server.requests[0])).toContain('Connectivity check')
  })

  it('settles at the first chunk without claiming usage the provider has not sent', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek', model: 'deepseek-flash' })

    // The minimal scripted completion answers with one text chunk, which is
    // where the probe settles; usage arrives after it and is therefore not
    // claimed rather than estimated.
    expect(result.usage).toBeUndefined()
    expect(server.requests[0]).toMatchObject({ model: 'deepseek-flash', stream: true })
  })

  it('reports a refused credential as the provider answer it is', async () => {
    const server = await mockServer([{
      status: 401,
      body: JSON.stringify({ error: { message: 'Incorrect API key provided' } }),
    }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek', model: 'deepseek-flash' })

    expect(result.ok).toBe(false)
    expect(result.failure?.code).toBe('AUTH')
    expect(result.failure?.message).toContain('Incorrect API key provided')
    expect(server.paths).toEqual(['/chat/completions'])
  })

  it('reports a server-side failure without blaming the credential', async () => {
    const server = await mockServer([{ status: 503, body: JSON.stringify({ error: { message: 'no capacity' } }) }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek', model: 'deepseek-flash' })

    expect(result).toMatchObject({ ok: false, failure: { code: 'SERVER' } })
    expect(result.failure?.message).toContain('no capacity')
  })

  it('surfaces the message for a model the route does not declare', async () => {
    const server = await mockServer([])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek', model: 'not-declared-anywhere' })

    expect(result).toMatchObject({ ok: false, model: 'not-declared-anywhere', failure: { code: 'UNKNOWN_MODEL' } })
    expect(result.failure?.message).toMatch(/has no configured model "not-declared-anywhere"/)
    expect(server.requests).toEqual([])
  })

  it('reports a missing credential as the configuration fault it is', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = new Context()
    Reflect.deleteProperty(process.env, 'PI_CONNECT_ABSENT')
    await ctx.plugin(LlmRuntime)
    await ctx.plugin(LlmPiAi, {
      providers: { deepseek: { apiKeyEnv: 'PI_CONNECT_ABSENT', baseURL: server.url } },
    })

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek', model: 'deepseek-flash' })

    expect(result).toMatchObject({ ok: false, failure: { code: 'MISSING_CREDENTIAL' } })
    expect(result.failure?.message).toContain('PI_CONNECT_ABSENT')
  })

  it('withdraws the offer when the plugin unloads', async () => {
    const server = await mockServer([{ events: textEvents }, { events: textEvents }])
    const ctx = new Context()
    await ctx.plugin(LlmRuntime)
    const fiber = await ctx.plugin(LlmPiAi, {})

    await expect(ctx.llm.testModel('llm-pi-ai', { baseURL: server.url, model: 'acme-large', apiKey: 'k' }))
      .resolves.toMatchObject({ ok: true })

    await fiber.dispose()

    await expect(ctx.llm.testModel('llm-pi-ai', { baseURL: server.url, model: 'acme-large', apiKey: 'k' }))
      .rejects.toMatchObject({ code: 'NO_TEST' })
  })
})

describe('draft-route model connectivity', () => {
  it('sends a typed key to the endpoint it was typed for, not the stored one', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = new Context()
    Reflect.deleteProperty(process.env, 'PI_CONNECT_KEY')
    await ctx.plugin(LlmRuntime)
    await ctx.plugin(LlmPiAi, {
      providers: { deepseek: { apiKeyEnv: 'PI_CONNECT_KEY', baseURL: server.url, headers: { 'X-Tenant': 'private-tenant' } } },
    })

    // The Models page keeps only a redacted descriptor for a stored key, so a
    // probe of the edited route has to carry the typed key itself. The route's
    // endpoint and deployment headers still come from its own profile.
    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek', model: 'deepseek-flash', apiKey: 'typed-key' })

    expect(result).toMatchObject({ ok: true })
    expect(server.paths).toEqual(['/chat/completions'])
    expect(server.headers[0]?.authorization).toBe('Bearer typed-key')
    expect(server.headers[0]?.['x-tenant']).toBe('private-tenant')
  })

  it('probes an endpoint the registry does not serve', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', {
      baseURL: `${server.url}/v1`,
      apiKey: 'probe-key',
      model: 'acme-large',
    })

    expect(result).toMatchObject({ ok: true, model: 'acme-large' })
    expect(server.paths).toEqual(['/v1/chat/completions'])
    expect(server.headers[0]?.authorization).toBe('Bearer probe-key')
  })

  it('asks a Messages endpoint through its own protocol and header', async () => {
    const server = await mockServer([{ events: anthropicTextEvents }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', {
      baseURL: server.url,
      api: 'anthropic-messages',
      apiKey: 'anthropic-key',
      model: 'claude-x',
    })

    expect(result).toMatchObject({ ok: true })
    expect(server.paths).toEqual(['/v1/messages?beta=true'])
    expect(server.headers[0]?.['x-api-key']).toBe('anthropic-key')
    expect(server.headers[0]?.authorization).toBeUndefined()
  })

  it('carries a configured route\'s protocol when the draft supplies only a key', async () => {
    const server = await mockServer([{ events: anthropicTextEvents }])
    const ctx = new Context()
    await ctx.plugin(LlmRuntime)
    await ctx.plugin(LlmPiAi, {
      providers: {
        'acme-gateway': {
          apiKeyEnv: 'PI_CONNECT_KEY',
          api: 'anthropic-messages',
          baseURL: server.url,
          models: [{ id: 'acme-large', contextWindow: 32_768 }],
        },
      },
    })

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'acme-gateway', model: 'acme-large', apiKey: 'typed' })

    expect(result.failure).toBeUndefined()
    expect(server.paths).toEqual(['/v1/messages?beta=true'])
    // The route's own catalog entry is the one probed, not a bare id.
    expect(JSON.stringify(server.requests[0])).toContain('acme-large')
  })

  it('refuses a route it can neither find in the catalog nor address', async () => {
    const ctx = await harness('http://127.0.0.1:9')

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'acme-gateway', model: 'acme-large', apiKey: 'k' })

    expect(result).toMatchObject({ ok: false, failure: { code: 'INVALID_TEST' } })
    expect(result.failure?.message).toMatch(/no endpoint to send it to/)
  })

  it('refuses a protocol this build cannot serve before any request', async () => {
    const server = await mockServer([])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', {
      baseURL: `${server.url}/v1`,
      api: 'bedrock',
      apiKey: 'k',
      model: 'acme-large',
    })

    expect(result).toMatchObject({ ok: false, failure: { code: 'INVALID_CONFIG' } })
    expect(result.failure?.message).toMatch(/cannot serve this model test/)
    expect(server.requests).toEqual([])
  })

  it('reports an unusable probe key without reaching the network', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', {
      baseURL: `${server.url}/v1`,
      apiKey: 'sk-\u{1F600}',
      model: 'acme-large',
    })

    expect(result).toMatchObject({ ok: false, failure: { code: 'INVALID_CREDENTIAL' } })
    expect(server.requests).toEqual([])
  })

  it('probes an installed catalog route at its own endpoint', async () => {
    // A route the plugin does not configure still answers: pi-ai ships its
    // endpoint and protocol, so a draft that names only the route is complete.
    const server = await mockServer([{ events: textEvents }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', {
      provider: 'acme-not-configured',
      baseURL: server.url,
      apiKey: 'k',
      model: 'acme-large',
    })

    expect(result).toMatchObject({ ok: true })
    expect(server.paths).toEqual(['/chat/completions'])
  })
})

describe('model connectivity cancellation', () => {
  it('reports a caller that cancels before dispatch as cancellation, not a refusal', async () => {
    const server = await mockServer([{ events: textEvents }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel(
      'llm-pi-ai',
      { provider: 'deepseek', model: 'deepseek-flash' },
      AbortSignal.abort('closed the dialog'),
    )

    expect(result).toMatchObject({ ok: false, failure: { code: 'ABORTED', message: 'model test aborted by caller' } })
    expect(server.requests).toEqual([])
  })

  it('reports a caller that cancels mid-response as cancellation, not a provider rejection', async () => {
    // The endpoint accepts the request and then says nothing, so the abort
    // lands on a live stream rather than on a request that never left.
    const server = await mockServer([{ events: [], holdOpen: true }])
    const ctx = await harness(server.url)
    const controller = new AbortController()
    const request: LlmModelTestRequest = { provider: 'deepseek', model: 'deepseek-flash' }

    const pending = ctx.llm.testModel('llm-pi-ai', request, controller.signal)
    await server.requestReceived
    controller.abort('closed the dialog')

    await expect(pending).resolves.toMatchObject({
      ok: false,
      failure: { code: 'ABORTED', message: 'model test aborted by caller' },
    })
  })

  it('cancels the provider request once the probe has answered', async () => {
    const server = await mockServer([{ events: textEvents, holdOpen: true }])
    const ctx = await harness(server.url)

    const result = await ctx.llm.testModel('llm-pi-ai', { provider: 'deepseek', model: 'deepseek-flash' })

    expect(result.ok).toBe(true)
    // The probe settled at the first chunk and abandoned the response it left
    // open, so the mock endpoint observed the disconnect.
    await expect.poll(() => server.closedResponses, { timeout: 2_000 }).toBe(1)
  })
})
