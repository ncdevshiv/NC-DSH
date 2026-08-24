/**
 * REAL-composition coverage: a test-only cordis.yml booted through the real
 * Loader + Include path mounts the whole opt-in browser stack — seam, moli
 * provider, tool consumer — so the model-visible surface registers exactly as
 * a profile overlay would assemble it, and a launch without a usable binary
 * fails loud through the assembled registry.
 */

import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { afterEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import Loader from '@deepseek-ai/cordis-plugin-loader'
import Include from '@deepseek-ai/cordis-plugin-include'
import { CallId } from '@deepseek-ai/dsh-llm'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import ToolRuntime, { type ToolExecutionResult } from '@deepseek-ai/dsh-tools'
import BrowserRuntime from '@deepseek-ai/dsh-browser'
import * as browserMoli from '@deepseek-ai/dsh-browser-moli'
import * as ToolBrowser from '@deepseek-ai/dsh-tool-browser'

let root: string | undefined
let context: Context | undefined

afterEach(async () => {
  await context?.fiber.dispose()
  context = undefined
  if (root !== undefined) await rm(root, { recursive: true, force: true })
  root = undefined
})

/** The nonexistent binary keeps the mounted provider unavailable without probing long. */
const ABSENT_BINARY = 'dsh-test-no-such-moli-binary'

async function loadBrowserYaml(): Promise<Context> {
  root = await mkdtemp(join(tmpdir(), 'dsh-tool-browser-loader-'))
  const configPath = join(root, 'cordis.yml')
  await writeFile(configPath, [
    "- name: '@deepseek-ai/dsh-system-prompt'",
    "- name: '@deepseek-ai/dsh-tools'",
    "- name: '@deepseek-ai/dsh-browser'",
    "- name: '@deepseek-ai/dsh-browser-moli'",
    '  config:',
    `    binaryPath: '${ABSENT_BINARY}'`,
    "- name: '@deepseek-ai/dsh-tool-browser'",
    '',
  ].join('\n'))

  context = new Context()
  context.baseUrl = pathToFileURL(root).href + '/'
  await context.plugin(Loader)
  context.loader.builtins.include = Include
  const modules = new Map<string, unknown>([
    ['@deepseek-ai/dsh-system-prompt', SystemPrompt],
    ['@deepseek-ai/dsh-tools', ToolRuntime],
    ['@deepseek-ai/dsh-browser', BrowserRuntime],
    ['@deepseek-ai/dsh-browser-moli', browserMoli],
    ['@deepseek-ai/dsh-tool-browser', ToolBrowser],
  ])
  context.loader.internal = {
    version: 'v2',
    async import(specifier: string) {
      if (!modules.has(specifier)) throw new Error(`unexpected Loader import: ${specifier}`)
      return modules.get(specifier)
    },
  } as unknown as NonNullable<typeof context.loader.internal>
  await context.loader.create({
    name: 'cordis:include',
    config: { path: pathToFileURL(configPath).href },
  })
  await context.loader.await()
  return context
}

function textOf(out: ToolExecutionResult): string {
  return out.content.map(block => block.type === 'text' ? block.text : '').join('')
}

describe('browser stack real Loader composition through cordis.yml', () => {
  it('mounts seam, provider, and consumer and registers exactly the five browser tools', async () => {
    const ctx = await loadBrowserYaml()

    const unloaded = [...ctx.loader.entries()]
      .filter(entry => entry.fiber === undefined && !entry.disabled)
      .map(entry => entry.options.name)
    expect(unloaded).toEqual([])
    for (const name of ['browser_navigate', 'browser_snapshot', 'browser_click', 'browser_type', 'browser_screenshot']) {
      expect(ctx.tools.get(name)?.name).toBe(name)
    }
    // Duplicate section names throw within a layer, so a rejected duplicate
    // proves the composed guidance section is present.
    expect(() => ctx.systemPrompt.section({ name: 'tool:browser', order: 0, text: 'duplicate probe' })).toThrow()
  })

  it('fails a model call loud and structured when the configured binary is absent', async () => {
    const ctx = await loadBrowserYaml()

    const out = await ctx.tools.execute({
      signal: new AbortController().signal,
      callId: CallId('loader-composition-1'),
      name: 'browser_snapshot',
      arguments: {},
    })
    expect(out.isError).toBe(true)
    // Auto-selection finds one registered provider (moli) whose availability
    // probe fails on the absent binary, so the whole chain reports the seam's
    // structured unavailability instead of hanging or misconfiguring silently.
    expect(textOf(out)).toContain('no usable browser provider is registered')
  })
})
