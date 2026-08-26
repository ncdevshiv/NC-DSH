/**
 * Adapter coverage over the real protocol stack: every stream here runs
 * through a spawned mock sidecar child speaking line-delimited JSON-RPC, so
 * framing, multiplexing, cancellation, and error mapping are exercised
 * end-to-end without network access.
 * @module adapter.spec
 */

import { fileURLToPath } from 'node:url'
import { afterAll, afterEach, describe, expect, it } from 'vitest'
import { MessageId } from '@deepseek-ai/dsh-llm'
import type { GenerateOptions, StreamChunk } from '@deepseek-ai/dsh-llm'
import { AiSdkAdapter } from '../src/adapter.ts'
import type { ResolvedRoute, ResolvedRouteSet } from '../src/adapter.ts'
import { AiSidecarClient } from '../src/sidecar.ts'
import { resolveRetryPolicy } from '@deepseek-ai/dsh-llm'

const MOCK_SIDECAR = fileURLToPath(new URL('./mock-sidecar.mjs', import.meta.url))

function route(overrides: Partial<ResolvedRoute> = {}): ResolvedRoute {
  return {
    id: 'deepseek-official',
    displayName: 'DeepSeek',
    apiKeyEnv: 'DEEPSEEK_API_KEY',
    baseURL: 'https://api.deepseek.com',
    api: undefined,
    models: [{ id: 'deepseek-v4-flash', name: 'DeepSeek-V4-Flash', contextWindow: 1_000_000 }],
    maxTokens: 256_000,
    defaultContextWindow: 1_000_000,
    reasoningEfforts: ['off', 'low', 'high', 'max'],
    defaultReasoningEffort: 'high',
    maxRequestImageBytes: 20 * 1024 * 1024,
    ...overrides,
  }
}

function routeSet(...routes: ResolvedRoute[]): ResolvedRouteSet {
  return { binaryPath: '/unused-by-adapter', routes: new Map(routes.map(entry => [entry.id, entry])) }
}

const clients: AiSidecarClient[] = []

afterEach(() => {
  while (clients.length > 0) clients.pop()!.dispose()
})

afterAll(() => {
  // Nothing persistent: every client disposes its own child in afterEach.
})

function harness(script: unknown[], options?: { fail?: boolean }): {
  adapter: AiSdkAdapter
} {
  process.env.MOCK_SIDECAR_SCRIPT = JSON.stringify(script)
  if (options?.fail === true) process.env.MOCK_SIDECAR_FAIL = '1'
  else delete process.env.MOCK_SIDECAR_FAIL
  const client = new AiSidecarClient(() => ({ command: process.execPath, args: [MOCK_SIDECAR] }))
  clients.push(client)
  const current = routeSet(route())
  const adapter = new AiSdkAdapter(
    {
      options: () => current,
      resolveApiKey: async () => 'test-key',
      resolveAttachments: () => undefined,
    },
    client,
    () => 5_000,
    () => resolveRetryPolicy(undefined, 'test'),
  )
  return { adapter }
}

function generateOptions(overrides: Partial<GenerateOptions> = {}): GenerateOptions {
  return {
    provider: 'deepseek-official',
    model: 'deepseek-v4-flash',
    messages: [{
      id: MessageId('m-1'),
      role: 'user',
      content: [{ type: 'text', text: 'hi' }],
      source: { kind: 'user' },
    }],
    ...overrides,
  }
}

async function collect(stream: AsyncIterable<StreamChunk>): Promise<StreamChunk[]> {
  const chunks: StreamChunk[] = []
  for await (const chunk of stream) chunks.push(chunk)
  return chunks
}

describe('AiSdkAdapter.stream', () => {
  it('streams a scripted text completion into correlated chunks', async () => {
    const { adapter } = harness([
      { type: 'text_delta', delta: 'he' },
      { type: 'text_delta', delta: 'llo' },
      { type: 'usage_update', usage: { input_tokens: 4, output_tokens: 2 } },
      { type: 'completed', finish_reason: 'stop' },
    ])
    const chunks = await collect(adapter.stream(generateOptions()))
    expect(chunks).toEqual([
      { type: 'block-start', index: 0, blockType: 'text' },
      { type: 'text-delta', index: 0, text: 'he' },
      { type: 'text-delta', index: 0, text: 'llo' },
      { type: 'block-end', index: 0, block: { type: 'text', text: 'hello' } },
      { type: 'usage', usage: { inputTokens: 4, outputTokens: 2 } },
      { type: 'finish', reason: { kind: 'stop' } },
    ])
  })

  it('maps a terminal sidecar failure onto the harness failure code', async () => {
    const { adapter } = harness([{ type: 'text_delta', delta: 'partial' }], { fail: true })
    const chunks = await collect(adapter.stream(generateOptions()))
    // An in-band provider failure is data: it arrives as the terminal finish
    // chunk with the typed kind mapped onto the harness vocabulary.
    expect(chunks.at(-1)).toEqual({
      type: 'finish',
      reason: { kind: 'error', failure: { message: 'slow down', code: 'RATE_LIMIT' } },
    })
    expect(chunks.some(chunk => chunk.type === 'block-end')).toBe(true)
  })

  it('settles a hung stream promptly when the caller aborts', async () => {
    const { adapter } = harness(['hang'])
    const controller = new AbortController()
    setTimeout(() => controller.abort(), 50)
    await expect(collect(adapter.stream(generateOptions({ signal: controller.signal }))))
      .rejects.toThrow(/aborted by caller/)
  })

  it('serves repeated streams over one child process', async () => {
    const { adapter } = harness([{ type: 'completed', finish_reason: 'stop' }])
    const first = await collect(adapter.stream(generateOptions()))
    const second = await collect(adapter.stream(generateOptions()))
    expect(first.at(-1)).toEqual({ type: 'finish', reason: { kind: 'stop' } })
    expect(second.at(-1)).toEqual({ type: 'finish', reason: { kind: 'stop' } })
  })

  it('rejects an unregistered provider route before any transport work', async () => {
    const client = new AiSidecarClient(() => ({ command: process.execPath, args: [MOCK_SIDECAR] }))
    clients.push(client)
    const empty = new AiSdkAdapter(
      { options: routeSet, resolveApiKey: async () => 'k', resolveAttachments: () => undefined },
      client,
      () => 5_000,
      () => resolveRetryPolicy(undefined, 'test'),
    )
    await expect(collect(empty.stream(generateOptions()))).rejects.toThrow(/no AI SDK route/)
  })
})

describe('AiSdkAdapter metadata surface', () => {
  it('exposes catalog models and resolved context facts without a child', async () => {
    const client = new AiSidecarClient(() => ({ command: process.execPath, args: [MOCK_SIDECAR] }))
    clients.push(client)
    const current = routeSet(route())
    const adapter = new AiSdkAdapter(
      { options: () => current, resolveApiKey: async () => 'k', resolveAttachments: () => undefined },
      client,
      () => 5_000,
      () => resolveRetryPolicy(undefined, 'test'),
    )
    await expect(adapter.listModels('deepseek-official')).resolves.toEqual([{
      provider: 'deepseek-official',
      id: 'deepseek-v4-flash',
      name: 'DeepSeek-V4-Flash',
      inputModalities: ['text'],
    }])
    await expect(adapter.resolveModel('deepseek-official', 'unknown-model')).resolves.toMatchObject({
      provider: 'deepseek-official',
      id: 'unknown-model',
      context: { contextWindow: 1_000_000 },
      defaultMaxTokens: 256_000,
    })
    expect(adapter.providerInfo('deepseek-official')).toEqual({
      id: 'deepseek-official',
      name: 'DeepSeek',
    })
  })
})

describe('spawn failure surfaces loudly', () => {
  it('reports an unusable sidecar command as a protocol error', async () => {
    process.env.MOCK_SIDECAR_SCRIPT = '[]'
    const client = new AiSidecarClient(() => ({
      command: fileURLToPath(new URL('./definitely-missing-sidecar.mjs', import.meta.url)),
      args: [],
    }))
    clients.push(client)
    // A missing executable emits spawn `error` without `exit`; the stream
    // must reject at launch instead of hanging until the request ceiling.
    const current = routeSet(route())
    const adapter = new AiSdkAdapter(
      { options: () => current, resolveApiKey: async () => 'k', resolveAttachments: () => undefined },
      client,
      () => 5_000,
      () => resolveRetryPolicy(undefined, 'test'),
    )
    await expect(collect(adapter.stream(generateOptions()))).rejects.toThrow(/ai-sidecar/)
  })
})

/** Resolves once the OS process has exited; rejects on a 10 s poll deadline. */
async function untilExited(pid: number): Promise<void> {
  const deadline = Date.now() + 10_000
  while (Date.now() < deadline) {
    try {
      process.kill(pid, 0)
    } catch {
      return
    }
    await new Promise(resolve => setTimeout(resolve, 25))
  }
  throw new Error(`sidecar child ${pid} did not exit within 10s`)
}

describe('AiSidecarClient lifecycle', () => {
  it('dispose() terminates the spawned child process', async () => {
    const client = new AiSidecarClient(() => ({ command: process.execPath, args: [MOCK_SIDECAR] }))
    clients.push(client)
    process.env.MOCK_SIDECAR_SCRIPT = '[]'
    delete process.env.MOCK_SIDECAR_FAIL_INIT
    const providers = await client.listProviders()
    expect(providers).toEqual([])
    const pid = client.pid
    expect(pid).toBeTypeOf('number')
    client.dispose()
    expect(client.pid).toBeUndefined()
    await untilExited(pid!)
  })

  it('a failed initialize kills its child and the next attempt spawns fresh', async () => {
    process.env.MOCK_SIDECAR_SCRIPT = '[]'
    process.env.MOCK_SIDECAR_FAIL_INIT = '1'
    const connection = (): { command: string; args: string[] } =>
      ({ command: process.execPath, args: [MOCK_SIDECAR] })
    const client = new AiSidecarClient(connection)
    clients.push(client)
    await expect(client.listProviders()).rejects.toThrow(/initialize refused/)
    const failedPid = client.pid
    expect(failedPid).toBeUndefined()
    if (failedPid !== undefined) await untilExited(failedPid)
    // The failed generation left nothing behind and cached nothing: the next
    // call spawns a new child that answers normally.
    delete process.env.MOCK_SIDECAR_FAIL_INIT
    await expect(client.listProviders()).resolves.toEqual([])
    expect(client.pid).toBeTypeOf('number')
    expect(client.pid).not.toBe(failedPid)
  })
})

describe('AiSidecarClient.discoverModels', () => {
  it('returns the endpoint-reported models for an unsaved draft', async () => {
    process.env.MOCK_SIDECAR_SCRIPT = '[]'
    delete process.env.MOCK_SIDECAR_FAIL_INIT
    const client = new AiSidecarClient(() => ({ command: process.execPath, args: [MOCK_SIDECAR] }))
    clients.push(client)
    await expect(client.discoverModels({
      apiKey: 'draft-key',
      baseURL: 'https://gateway.example/v1',
    })).resolves.toEqual([
      {
        id: 'discovered-small',
        name: 'Discovered Small',
        context_window: 8192,
        max_output_tokens: 2048,
        capabilities: { input_modalities: ['text'] },
      },
      { id: 'discovered-large', context_window: 200000, max_output_tokens: 32768 },
    ])
  })

  it('surfaces the sidecar contract refusal for an OpenAI-compatible draft without an endpoint', async () => {
    process.env.MOCK_SIDECAR_SCRIPT = '[]'
    delete process.env.MOCK_SIDECAR_FAIL_INIT
    const client = new AiSidecarClient(() => ({ command: process.execPath, args: [MOCK_SIDECAR] }))
    clients.push(client)
    await expect(client.discoverModels({ apiKey: 'k' }))
      .rejects.toThrow(/missing `base_url`/)
  })
})
