/**
 * Real Loader composition: a test-only cordis.yml boots the zvec provider,
 * the workspace tools, and the session services, then exercises the tools
 * through `ctx.tools.execute`.
 */

import { afterEach, describe, expect, it } from 'vitest'
import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Context } from '@deepseek-ai/cordis'
import Loader from '@deepseek-ai/cordis-plugin-loader'
import { provideWorkingDirectoryFixture } from '@deepseek-ai/dsh-agent-loop-testkit'
import { ToolCallId } from '@deepseek-ai/dsh-llm'
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session'
import SessionProjectionRegistry from '@deepseek-ai/dsh-session-projection'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import ToolRuntime from '@deepseek-ai/dsh-tools'
import ZvecRetrievalService from '@deepseek-ai/dsh-retrieval-zvec'
import * as ToolRetrieval from '@deepseek-ai/dsh-tool-retrieval'

const directories: string[] = []
const signal = new AbortController().signal

afterEach(async () => {
  for (const directory of directories.splice(0)) {
    await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 })
  }
})

async function workspace(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'dsh-retrieval-loader-'))
  directories.push(directory)
  return directory
}

function text(result: { content: { type: string; text?: string }[] }): string {
  return result.content.filter(block => block.type === 'text').map(block => block.text).join('')
}

describe('retrieval real Loader composition', () => {
  it('boots a cordis.yml tree and serves workspace search and index', async () => {
    const root = await workspace()
    const configPath = join(root, 'cordis.yml')
    await writeFile(configPath, [
      '- id: session-projection',
      "  name: '@deepseek-ai/dsh-session-projection'",
      '- id: session',
      "  name: '@deepseek-ai/dsh-session'",
      '- id: working-directory',
      "  name: '@deepseek-ai/dsh-working-directory'",
      '- id: system-prompt',
      "  name: '@deepseek-ai/dsh-system-prompt'",
      '- id: tools',
      "  name: '@deepseek-ai/dsh-tools'",
      '- id: retrieval-zvec',
      "  name: '@deepseek-ai/dsh-retrieval-zvec'",
      '  config:',
      '    embedding: false',
      "    dataDir: '.dsh'",
      '- id: tool-retrieval',
      "  name: '@deepseek-ai/dsh-tool-retrieval'",
      '',
    ].join('\n'))

    const ctx = new Context()
    const loader = Object.create(Loader.prototype) as Loader
    const unwrap = (module: Record<string, unknown>): Parameters<Context['plugin']>[0] =>
      loader.unwrapExports(module) as Parameters<Context['plugin']>[0]
    await ctx.plugin(unwrap(SessionProjectionRegistry as unknown as Record<string, unknown>))
    await ctx.plugin(unwrap(SessionStore as unknown as Record<string, unknown>))
    provideWorkingDirectoryFixture(ctx, root)
    await ctx.plugin(unwrap(SystemPrompt as unknown as Record<string, unknown>))
    await ctx.plugin(unwrap(ToolRuntime as unknown as Record<string, unknown>))
    await ctx.plugin(unwrap(ZvecRetrievalService as unknown as Record<string, unknown>), { embedding: false, dataDir: '.dsh' })
    await ctx.plugin(ToolRetrieval)
    void configPath

    const session = ctx.sessions.create(SessionId('loader-retrieval'), { meta: { createdAt: 1, cwd: root, isSeeded: false } })
    const agent = { session } as never

    const indexed = await ctx.tools.execute({
      signal,
      callId: ToolCallId('loader-index'),
      name: 'workspace_index',
      arguments: { documents: [{ id: 'doc-a', text: 'loader needle text', path: 'src/a.ts' }] },
      agent,
    })
    expect(indexed.isError).toBe(false)
    expect(text(indexed)).toBe('Indexed 1 workspace document.')

    await ctx.retrieval.flush()
    const found = await ctx.tools.execute({
      signal,
      callId: ToolCallId('loader-search'),
      name: 'workspace_search',
      arguments: { query: 'needle' },
      agent,
    })
    expect(found.isError).toBe(false)
    expect(text(found)).toContain('src/a.ts')
    await ctx.fiber.dispose()
  })
})
