/**
 * Real-composition guard: LlmRuntime, settings-file, credentials-local, and
 * llm-ai-sdk boot from a test-only cordis.yml through the actual Loader +
 * Include path. Route registration, the configurable-provider directory, and
 * settings-driven route replacement are verified without spawning the sidecar
 * child (no credential or binary is needed to observe topology).
 */

import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import Loader from '@deepseek-ai/cordis-plugin-loader'
import Include from '@deepseek-ai/cordis-plugin-include'
import LlmRuntime from '@deepseek-ai/dsh-llm'
import LocalCredentialProvider from '@deepseek-ai/dsh-credentials-local'
import FileSettingsProvider from '@deepseek-ai/dsh-settings-file'
import * as LlmAiSdk from '@deepseek-ai/dsh-llm-ai-sdk'

let root: string | undefined
let context: Context | undefined

afterEach(async () => {
  await context?.fiber.dispose()
  context = undefined
  if (root !== undefined) await rm(root, { recursive: true, force: true })
  root = undefined
  vi.unstubAllEnvs()
})

interface LoadOptions {
  providers?: Record<string, unknown>
}

async function loadComposition(options: LoadOptions = {}): Promise<{ ctx: Context; settingsPath: string }> {
  root = await mkdtemp(join(tmpdir(), 'dsh-llm-ai-sdk-composition-'))
  vi.stubEnv('DSH_HOME', root)
  const settingsPath = join(root, 'settings.yaml')
  const credentialsPath = join(root, '.credentials.yaml')
  await writeFile(settingsPath, '# personal settings\n')
  await writeFile(credentialsPath, 'DEEPSEEK_API_KEY: composition-key\n', { mode: 0o600 })

  const configPath = join(root, 'cordis.yml')
  const providerRows = options.providers === undefined ? [] : [
    '    providers:',
    ...Object.entries(options.providers).flatMap(([route, profile]) => [
      `      ${JSON.stringify(route)}:`,
      `        baseURL: https://${route}.example`,
      ...Object.entries(profile ?? {}).map(([key, value]) =>
        `        ${key}: ${JSON.stringify(value)}`),
    ]),
  ]
  await writeFile(configPath, [
    '- id: llm',
    "  name: 'test-llm-service'",
    '- id: settings',
    "  name: '@deepseek-ai/dsh-settings-file'",
    '  config:',
    `    path: ${JSON.stringify(settingsPath)}`,
    '    debounceMs: 10',
    '- id: credentials',
    "  name: '@deepseek-ai/dsh-credentials-local'",
    '  config:',
    `    path: ${JSON.stringify(credentialsPath)}`,
    '    debounceMs: 10',
    '- id: llm-ai-sdk',
    "  name: '@deepseek-ai/dsh-llm-ai-sdk'",
    '  config:',
    '    binaryPath: /usr/bin/ai-sidecar',
    ...providerRows,
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
  return { ctx, settingsPath }
}

describe('llm-ai-sdk composition', () => {
  it('registers the default route when no providers are configured', async () => {
    const { ctx } = await loadComposition()
    expect(ctx.llm.listProviders()).toEqual([{ id: 'deepseek-official', name: 'DeepSeek' }])
    expect(ctx.llm.listConfigurableProviders()).toEqual([{
      provider: 'deepseek-official',
      displayName: 'DeepSeek',
      settingsNs: 'llm-ai-sdk',
      settingsPath: ['providers', 'deepseek-official'],
      declared: true,
    }])
  })

  it('replaces routes atomically when the settings section changes', async () => {
    const { ctx, settingsPath } = await loadComposition({
      providers: { 'openrouter': {}, 'ollama-local': {} },
    })
    expect(ctx.llm.listProviders().map(provider => provider.id).sort())
      .toEqual(['ollama-local', 'openrouter'])
    // Fixtures declare no catalog rows: each route serves its own (empty)
    // advisory list, and no route answers for a sibling.
    await expect(ctx.llm.listModels('openrouter')).resolves.toEqual([])
    await expect(ctx.llm.listModels('ollama-local')).resolves.toEqual([])

    await writeFile(settingsPath, 'llm-ai-sdk:\n  providers:\n    deepseek-official:\n      baseURL: https://api.example\n')
    // The watcher debounces, then the section re-resolves and replaces the
    // registry; wait it out the way the settings-file suite does. The user
    // layer merges over the composition base per key, so the declared routes
    // stay and the stored route joins them.
    await vi.waitFor(() => {
      const ids = ctx.llm.listProviders().map(provider => provider.id).sort()
      expect(ids).toEqual(['deepseek-official', 'ollama-local', 'openrouter'])
    }, { timeout: 5_000 })
  })

  it('answers discovery from the advisory catalog of a known route without a sidecar call', async () => {
    const { ctx } = await loadComposition({
      providers: {
        'acme-gateway': {
          models: [{ id: 'acme-mini', name: 'Acme Mini', contextWindow: 32_000 }],
        },
      },
    })
    // A draft naming only the route asks "what do you know" — answered from
    // the resolved profile's catalog; no sidecar binary exists in this boot.
    // The schema materializes the declared text-only modality default.
    await expect(ctx.llm.discoverModels('llm-ai-sdk', { provider: 'acme-gateway' }))
      .resolves.toEqual([
        { id: 'acme-mini', name: 'Acme Mini', contextWindow: 32_000, inputModalities: ['text'] },
      ])
  })

  it('keeps the last good resolution after an invalid settings edit', async () => {
    const { ctx, settingsPath } = await loadComposition({ providers: { 'gateway-a': {} } })
    expect(ctx.llm.listProviders().map(provider => provider.id)).toEqual(['gateway-a'])
    await writeFile(settingsPath, 'llm-ai-sdk:\n  streamIdleTimeoutMs: -5\n')
    // Give the debounced watcher its window; the invalid snapshot is refused
    // wholesale so the previous generation, route intact, stays authoritative.
    await new Promise((resolve) => { setTimeout(resolve, 200) })
    expect(ctx.llm.listProviders().map(provider => provider.id)).toEqual(['gateway-a'])
  })
})
