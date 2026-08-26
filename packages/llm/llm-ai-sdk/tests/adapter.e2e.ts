/**
 * True end-to-end: harness composition → llm-ai-sdk adapter → the REAL
 * `ai-sidecar` binary (Rust AI SDK) → its OpenAI-compatible provider wire →
 * a local scripted chat-completions HTTP server, asserting the assembled
 * chunks on the harness side.
 *
 * Self-skips unless $DSH_AI_SDK_SIDECAR names a built `ai-sidecar` executable
 * (the release build of F:\alisia\ai-sdk: `cargo build --release -p
 * ai-sidecar`), matching the real-API e2e skip convention.
 */

import { createServer, type Server } from 'node:http'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import Loader from '@deepseek-ai/cordis-plugin-loader'
import Include from '@deepseek-ai/cordis-plugin-include'
import LlmRuntime from '@deepseek-ai/dsh-llm'
import type { StreamChunk } from '@deepseek-ai/dsh-llm'
import LocalCredentialProvider from '@deepseek-ai/dsh-credentials-local'
import FileSettingsProvider from '@deepseek-ai/dsh-settings-file'
import * as LlmAiSdk from '@deepseek-ai/dsh-llm-ai-sdk'

const SIDECAR = process.env.DSH_AI_SDK_SIDECAR

let server: Server | undefined
let requests: { path: string; authorization: string | undefined; body: Record<string, unknown> }[] = []

beforeAll(async () => {
  if (SIDECAR === undefined) return
  await new Promise<void>((resolve) => {
    server = createServer((request, response) => {
      // The OpenAI-compatible listing endpoint the discovery flow interrogates.
      if (request.method === 'GET' && (request.url ?? '').endsWith('/models')) {
        requests.push({ path: request.url ?? '/', authorization: request.headers.authorization, body: {} })
        response.writeHead(200, { 'content-type': 'application/json' })
        response.end(JSON.stringify({
          object: 'list',
          data: [
            { id: 'e2e-model', owned_by: 'e2e' },
            { id: 'e2e-vision', owned_by: 'e2e' },
          ],
        }))
        return
      }
      let raw = ''
      request.on('data', (chunk) => { raw += String(chunk) })
      request.on('end', () => {
        let body: Record<string, unknown> = {}
        try { body = JSON.parse(raw) as Record<string, unknown> } catch { /* keep empty */ }
        requests.push({
          path: request.url ?? '/',
          authorization: request.headers.authorization,
          body,
        })
        response.writeHead(200, { 'content-type': 'text/event-stream' })
        const events = [
          'data: {"id":"c1","choices":[{"index":0,"delta":{"role":"assistant","content":"he"}}]}\n\n',
          'data: {"id":"c1","choices":[{"index":0,"delta":{"content":"llo"}}]}\n\n',
          'data: {"id":"c1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":2}}\n\n',
          'data: [DONE]\n\n',
        ]
        for (const event of events) response.write(event)
        response.end()
      })
    })
    server.listen(0, '127.0.0.1', () => resolve())
  })
})

afterAll(async () => {
  await new Promise<void>((resolve, reject) => {
    if (server === undefined) return resolve()
    server.close(error => error === undefined ? resolve() : reject(error))
  })
})

let root: string | undefined
let context: Context | undefined

afterEach(async () => {
  await context?.fiber.dispose()
  context = undefined
  if (root !== undefined) await rm(root, { recursive: true, force: true })
  root = undefined
  requests = []
  vi.unstubAllEnvs()
})

describe.skipIf(SIDECAR === undefined)('real sidecar end-to-end', () => {
  it('streams a completion from the Rust sidecar through its OpenAI-compatible wire', async () => {
    const endpoint = (server as Server).address() as { port: number }
    const address = `http://127.0.0.1:${endpoint.port}`
    root = await mkdtemp(join(tmpdir(), 'dsh-llm-ai-sdk-e2e-'))
    vi.stubEnv('DSH_HOME', root)
    const credentialsPath = join(root, '.credentials.yaml')
    await writeFile(credentialsPath, 'DEEPSEEK_API_KEY: e2e-key\n', { mode: 0o600 })

    const configPath = join(root, 'cordis.yml')
    await writeFile(configPath, [
      '- id: llm',
      "  name: 'test-llm-service'",
      '- id: credentials',
      "  name: '@deepseek-ai/dsh-credentials-local'",
      '  config:',
      `    path: ${JSON.stringify(credentialsPath)}`,
      '    debounceMs: 10',
      '- id: llm-ai-sdk',
      "  name: '@deepseek-ai/dsh-llm-ai-sdk'",
      '  config:',
      `    binaryPath: ${JSON.stringify(SIDECAR!)}`,
      '    providers:',
      '      deepseek-official:',
      '        displayName: DeepSeek',
      '        apiKeyEnv: DEEPSEEK_API_KEY',
      `        baseURL: ${JSON.stringify(address)}`,
      '',
    ].join('\n'))

    const ctx = new Context()
    context = ctx
    ctx.baseUrl = pathToFileURL(root).href + '/'
    await ctx.plugin(Loader)
    ctx.loader.builtins.include = Include
    const modules = new Map<string, unknown>([
      ['test-llm-service', LlmRuntime],
      ['@deepseek-ai/dsh-settings-file', FileSettingsProvider],
      ['@deepseek-ai/dsh-credentials-local', LocalCredentialProvider],
      ['@deepseek-ai/dsh-llm-ai-sdk', LlmAiSdk],
    ])
    ctx.loader.builtins.include = Include
    ;(ctx.loader as unknown as { internal?: unknown }).internal = {
      version: 'v2',
      async import(specifier: string) {
        if (!modules.has(specifier)) throw new Error(`unexpected Loader import: ${specifier}`)
        return modules.get(specifier)
      },
    }
    const id = await ctx.loader.create({
      name: 'cordis:include',
      config: { path: pathToFileURL(configPath).href, patches: [] },
    })
    void id
    await ctx.loader.await()

    const chunks: StreamChunk[] = []
    for await (const chunk of ctx.llm.stream({
      provider: 'deepseek-official',
      model: 'e2e-model',
      messages: [{
        id: 'm-1' as never,
        role: 'user' as const,
        content: [{ type: 'text' as const, text: 'hello' }],
        source: { kind: 'user' as const },
      }],
    })) {
      chunks.push(chunk)
    }

    expect(chunks.map(chunk => chunk.type)).toEqual([
      'block-start', 'text-delta', 'text-delta', 'block-end', 'usage', 'finish',
    ])
    const text = chunks
      .filter((chunk): chunk is Extract<StreamChunk, { type: 'text-delta' }> => chunk.type === 'text-delta')
      .map(chunk => chunk.text)
      .join('')
    expect(text).toBe('hello')

    // The credential crossed the sidecar boundary exactly once per generation,
    // and the wire request reached the scripted endpoint with the bearer key.
    expect(requests.length).toBe(1)
    const first = requests[0]
    expect(first?.authorization).toBe('Bearer e2e-key')
  }, 30_000)

  it('discovers models on an unsaved endpoint through the real sidecar', async () => {
    const endpoint = (server as Server).address() as { port: number }
    const address = `http://127.0.0.1:${endpoint.port}`
    root = await mkdtemp(join(tmpdir(), 'dsh-llm-ai-sdk-e2e-'))
    vi.stubEnv('DSH_HOME', root)
    const credentialsPath = join(root, '.credentials.yaml')
    await writeFile(credentialsPath, 'DEEPSEEK_API_KEY: e2e-key\n', { mode: 0o600 })

    const configPath = join(root, 'cordis.yml')
    await writeFile(configPath, [
      '- id: llm',
      "  name: 'test-llm-service'",
      '- id: credentials',
      "  name: '@deepseek-ai/dsh-credentials-local'",
      '  config:',
      `    path: ${JSON.stringify(credentialsPath)}`,
      '    debounceMs: 10',
      '- id: llm-ai-sdk',
      "  name: '@deepseek-ai/dsh-llm-ai-sdk'",
      '  config:',
      `    binaryPath: ${JSON.stringify(SIDECAR!)}`,
      '',
    ].join('\n'))

    const ctx = new Context()
    context = ctx
    ctx.baseUrl = pathToFileURL(root).href + '/'
    await ctx.plugin(Loader)
    ctx.loader.builtins.include = Include
    const modules = new Map<string, unknown>([
      ['test-llm-service', LlmRuntime],
      ['@deepseek-ai/dsh-settings-file', FileSettingsProvider],
      ['@deepseek-ai/dsh-credentials-local', LocalCredentialProvider],
      ['@deepseek-ai/dsh-llm-ai-sdk', LlmAiSdk],
    ])
    ;(ctx.loader as unknown as { internal?: unknown }).internal = {
      version: 'v2',
      async import(specifier: string) {
        if (!modules.has(specifier)) throw new Error(`unexpected Loader import: ${specifier}`)
        return modules.get(specifier)
      },
    }
    await ctx.loader.create({ name: 'cordis:include', config: { path: pathToFileURL(configPath).href, patches: [] } })
    await ctx.loader.await()

    // The draft names only the endpoint; the stored credential of the named
    // route authenticates the interrogation, exactly as the Models page sends
    // it. The reply is candidate metadata in endpoint order; the SDK may
    // enrich each row with its own catalog defaults.
    const models = await ctx.llm.discoverModels('llm-ai-sdk', {
      provider: 'deepseek-official',
      baseURL: address,
    })
    expect(models.map(model => model.id)).toEqual(['e2e-model', 'e2e-vision'])
    expect(models.every(model => typeof model.id === 'string' && model.id.length > 0)).toBe(true)

    // The listing request carried the stored key as a bearer token.
    const listing = requests.find(request => (request.path ?? '').endsWith('/models'))
    expect(listing?.authorization).toBe('Bearer e2e-key')
  }, 30_000)
})
