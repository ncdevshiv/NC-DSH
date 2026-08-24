/**
 * Integration: the real tool registry (`dsh-tools`), system prompt, browser
 * seam (`dsh-browser`), a scripted in-memory `BrowserProvider`, the shared
 * session holder, and the real `dsh-tool-browser` tools — exercised through
 * `ctx.tools.execute()`; nothing bypasses the registry. The provider's
 * "browser" is an in-memory page model, so no process or network runs.
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import { CallId } from '@deepseek-ai/dsh-llm'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import ToolRuntime, { type ToolExecutionResult } from '@deepseek-ai/dsh-tools'
import BrowserRuntime from '@deepseek-ai/dsh-browser'
import type { BrowserProvider, BrowserSession } from '@deepseek-ai/dsh-browser'
import * as TimeoutPolicy from '@deepseek-ai/dsh-tool-call-timeout-policy'
import * as ToolBrowser from '@deepseek-ai/dsh-tool-browser'

const testToolSignal = new AbortController().signal

/** An in-memory page: one URL, title, content, and interaction bookkeeping. */
function makeScriptedSession(): BrowserSession & { calls: string[] } {
  const calls: string[] = []
  const state = () => ({ url: 'https://example.test/page', title: 'Example', content: 'Hello browser world' })
  return {
    calls,
    navigate: (request) => {
      calls.push(`navigate:${request.url}`)
      return Promise.resolve(state())
    },
    snapshot: () => {
      calls.push('snapshot')
      return Promise.resolve(state())
    },
    click: (request) => {
      if (request.selector === '#absent') return Promise.reject(new Error('no element matched the given CSS selector'))
      calls.push(`click:${request.selector}`)
      return Promise.resolve(state())
    },
    type: (request) => {
      calls.push(`type:${request.selector}:${request.text}:${request.submit === true}`)
      return Promise.resolve(state())
    },
    screenshot: () => {
      calls.push('screenshot')
      return Promise.resolve({ mediaType: 'image/png' as const, data: new Uint8Array([0x68, 0x69]) })
    },
    close: () => {
      calls.push('close')
      return Promise.resolve()
    },
  }
}

/** Provider handing out one shared scripted session, recording launches. */
function makeScriptedProvider(session: BrowserSession): BrowserProvider & { launchCount(): number } {
  let launched = 0
  return {
    id: 'scripted',
    available: () => true,
    launch: () => {
      launched += 1
      return Promise.resolve(session)
    },
    launchCount: () => launched,
  }
}

let ctx: Context
let fiber: Awaited<ReturnType<Context['plugin']>>
let scripted: ReturnType<typeof makeScriptedSession>
let provider: ReturnType<typeof makeScriptedProvider>
let counter = 0

beforeEach(async () => {
  scripted = makeScriptedSession()
  provider = makeScriptedProvider(scripted)
  ctx = new Context()
  await ctx.plugin(SystemPrompt)
  await ctx.plugin(ToolRuntime)
  await ctx.plugin(BrowserRuntime, { provider: provider.id })
  // Registration is itself an effect on the runtime's own fiber; the fresh
  // per-test context tears it down with the test.
  ctx.browser.registerProvider(provider)
  // Shipped deployment shape: per-tool budgets come from config and are
  // enforced by the zero-config timeout-policy plugin.
  await ctx.plugin(TimeoutPolicy)
  fiber = await ctx.plugin(ToolBrowser)
})

afterEach(async () => {
  await fiber.dispose()
})

function call(name: string, args: unknown): Promise<ToolExecutionResult> {
  counter += 1
  return ctx.tools.execute({ signal: testToolSignal, callId: CallId(`call-${counter}`), name, arguments: args })
}

function textOf(out: ToolExecutionResult): string {
  return out.content.map(block => block.type === 'text' ? block.text : '').join('')
}

describe('browser tools over the real registry', () => {
  it('registers exactly the five enabled tools plus their guidance section', () => {
    for (const name of ['browser_navigate', 'browser_snapshot', 'browser_click', 'browser_type', 'browser_screenshot']) {
      expect(ctx.tools.get(name)?.name).toBe(name)
    }
    // Duplicate section names throw within a layer, so a rejected duplicate
    // proves the suite's guidance section is present.
    expect(() => ctx.systemPrompt.section({ name: 'tool:browser', order: 0, text: 'duplicate probe' })).toThrow()
  })

  it('navigates through the shared scripted session', async () => {
    const out = await call('browser_navigate', { url: 'https://example.test/page' })
    expect(out.isError).toBe(false)
    expect(textOf(out)).toContain('Navigated to https://example.test/page')
    expect(textOf(out)).toContain('Hello browser world')
    expect(scripted.calls).toEqual(['navigate:https://example.test/page'])
    expect(provider.launchCount()).toBe(1)
  })

  it('snapshots without launching a second session', async () => {
    await call('browser_snapshot', {})
    await call('browser_snapshot', {})
    expect(scripted.calls).toEqual(['snapshot', 'snapshot'])
    expect(provider.launchCount()).toBe(1)
  })

  it('clicks and types by selector', async () => {
    await call('browser_click', { selector: '#submit' })
    await call('browser_type', { selector: '#q', text: 'hello', submit: true })
    expect(scripted.calls).toEqual(['click:#submit', 'type:#q:hello:true'])
  })

  it('surfaces a structured element-not-found error through the registry', async () => {
    const out = await call('browser_click', { selector: '#absent' })
    expect(out.isError).toBe(true)
    expect(textOf(out)).toContain('no element matched the given CSS selector')
  })

  it('saves the screenshot PNG and reports its path and size', async () => {
    const out = await call('browser_screenshot', { full_page: true })
    expect(out.isError).toBe(false)
    const text = textOf(out)
    expect(text).toContain('<type>image/png</type>')
    expect(text).toMatch(/<path>.+dsh-tool-browser-.+\.png<\/path>/)
    expect(text).toContain('2 bytes')
  })

  it('closes the underlying session when the plugin fiber is disposed (HMR safety)', async () => {
    await call('browser_snapshot', {})
    await fiber.dispose()
    expect(scripted.calls).toContain('close')
  })

  it('rejects blank required arguments before any session work', async () => {
    const out = await call('browser_navigate', { url: '   ' })
    expect(out.isError).toBe(true)
    expect(textOf(out)).toContain('url must be a non-empty string')
    expect(scripted.calls).toEqual([])
  })

  it('attaches cooperative timeout budgets to every definition', () => {
    expect(ctx.tools.get('browser_navigate')?.timeoutMs).toBe(30_000)
    expect(ctx.tools.get('browser_snapshot')?.timeoutMs).toBe(15_000)
    expect(ctx.tools.get('browser_screenshot')?.timeoutMs).toBe(15_000)
  })
})

describe('tool-browser enablement config', () => {
  it('registers nothing beyond guidance when every toggle is off', async () => {
    const local = new Context()
    await local.plugin(SystemPrompt)
    await local.plugin(ToolRuntime)
    await local.plugin(BrowserRuntime, { provider: provider.id })
    await local.plugin(ToolBrowser, { navigate: false, snapshot: false, click: false, typing: false, screenshot: false })
    for (const name of ['browser_navigate', 'browser_snapshot', 'browser_click', 'browser_type', 'browser_screenshot']) {
      expect(local.tools.get(name)).toBeUndefined()
    }
    // With every toggle off the guidance section is absent, so the same name
    // registers cleanly instead of colliding.
    expect(() => local.systemPrompt.section({ name: 'tool:browser', order: 0, text: 'probe' })).not.toThrow()
  })
})
