/**
 * The five model-facing browser tools over one shared session. Each module
 * function owns its schema, validation, and output rendering; the seam owns
 * interaction; the shared session holder owns launch and serialization.
 * Timeouts are deployment policy attached as `ToolDefinition.timeoutMs`, never
 * a model argument.
 * @module @deepseek-ai/dsh-tool-browser/tools
 */

import { writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Context } from '@deepseek-ai/cordis'
import { defineTool } from '@deepseek-ai/dsh-tools'
import type { BrowserPageState, BrowserSession } from '@deepseek-ai/dsh-browser'
import type {} from '@deepseek-ai/dsh-system-prompt'
import type { SharedBrowserSession } from './session-holder.ts'
import { assertNonBlank, formatPageState, formatScreenshotOutput } from './output.ts'

/** Canonical page-state tool outcome: exactly the seam's state fields. */
export type PageToolValue = BrowserPageState

/** Canonical screenshot tool outcome. */
export interface ScreenshotToolValue {
  /** Absolute path of the saved PNG. */
  path: string
  /** Encoded size in bytes. */
  bytes: number
}

const PAGE_STATE_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  properties: {
    url: { type: 'string', required: true },
    title: { type: 'string' },
    content: { type: 'string' },
  },
} as const

function pageOutput(verb: string, maxOutputChars: number) {
  return {
    schema: PAGE_STATE_SCHEMA,
    render: (_args: unknown, value: PageToolValue) => [{ type: 'text' as const, text: formatPageState(verb, value, maxOutputChars) }],
  }
}

/**
 * Register `browser_navigate`: load a URL and return the resulting page state.
 *
 * @param ctx - registration scope; effect-scoped like every registration.
 * @param session - the plugin's shared serialized session.
 * @param timeoutMs - cooperative tool-call budget for navigation.
 * @param maxOutputChars - cap on the complete rendered output.
 */
export function applyBrowserNavigateTool(ctx: Context, session: SharedBrowserSession, timeoutMs: number, maxOutputChars: number): void {
  ctx.tools.register(defineTool({
    name: 'browser_navigate',
    description: 'Navigate the browser to an HTTP(S) URL and return the loaded page state (url, title, text content).',
    parameters: {
      url: { type: 'string', required: true, description: 'The HTTP(S) URL to navigate to.' },
    },
    output: pageOutput('Navigated to', maxOutputChars),
    timeoutMs,
    // One shared stateful session: operations are sequential by design.
    isConcurrencySafe: () => false,
    async execute(args, exec) {
      const url = assertNonBlank('url', args.url)
      return session.run(browser => browser.navigate({ url }, exec.signal))
    },
  }))
}

/**
 * Register `browser_snapshot`: read the current page without navigating.
 *
 * @param ctx - registration scope.
 * @param session - the shared serialized session.
 * @param timeoutMs - cooperative tool-call budget.
 * @param maxOutputChars - cap on the complete rendered output.
 */
export function applyBrowserSnapshotTool(ctx: Context, session: SharedBrowserSession, timeoutMs: number, maxOutputChars: number): void {
  ctx.tools.register(defineTool({
    name: 'browser_snapshot',
    description: 'Read the browser\'s current page state (url, title, text content) without navigating.',
    parameters: {},
    output: pageOutput('Snapshot of', maxOutputChars),
    timeoutMs,
    isConcurrencySafe: () => false,
    async execute(_args, exec) {
      return session.run((browser: BrowserSession) => browser.snapshot(exec.signal))
    },
  }))
}

/**
 * Register `browser_click`: click the first match of a CSS selector.
 *
 * @param ctx - registration scope.
 * @param session - the shared serialized session.
 * @param timeoutMs - cooperative tool-call budget.
 * @param maxOutputChars - cap on the complete rendered output.
 */
export function applyBrowserClickTool(ctx: Context, session: SharedBrowserSession, timeoutMs: number, maxOutputChars: number): void {
  ctx.tools.register(defineTool({
    name: 'browser_click',
    description: 'Click the first element matching a CSS selector on the browser\'s current page and return the resulting page state.',
    parameters: {
      selector: { type: 'string', required: true, description: 'CSS selector of the element to click.' },
    },
    output: pageOutput('Clicked', maxOutputChars),
    timeoutMs,
    isConcurrencySafe: () => false,
    async execute(args, exec) {
      const selector = assertNonBlank('selector', args.selector)
      return session.run(browser => browser.click({ selector }, exec.signal))
    },
  }))
}

/**
 * Register `browser_type`: fill a CSS-selector-matched input, optionally
 * submitting with Enter.
 *
 * @param ctx - registration scope.
 * @param session - the shared serialized session.
 * @param timeoutMs - cooperative tool-call budget.
 * @param maxOutputChars - cap on the complete rendered output.
 */
export function applyBrowserTypeTool(ctx: Context, session: SharedBrowserSession, timeoutMs: number, maxOutputChars: number): void {
  ctx.tools.register(defineTool({
    name: 'browser_type',
    description: 'Type text into the first input matching a CSS selector on the browser\'s current page, optionally pressing Enter, and return the resulting page state.',
    parameters: {
      selector: { type: 'string', required: true, description: 'CSS selector of the input to fill.' },
      text: { type: 'string', required: true, description: 'Text to enter after clearing the existing value.' },
      submit: { type: 'boolean', description: 'Press Enter after typing.' },
    },
    output: pageOutput('Typed into', maxOutputChars),
    timeoutMs,
    isConcurrencySafe: () => false,
    async execute(args, exec) {
      const selector = assertNonBlank('selector', args.selector)
      const text = assertNonBlank('text', args.text)
      return session.run(browser => browser.type({ selector, text, ...(args.submit === true ? { submit: true } : {}) }, exec.signal))
    },
  }))
}

let screenshotCounter = 0

/**
 * Register `browser_screenshot`: capture the current page as a PNG saved under
 * the OS temp directory; inline pixel delivery waits on attachment-backed
 * image blocks (documented deferred work).
 *
 * @param ctx - registration scope.
 * @param session - the shared serialized session.
 * @param timeoutMs - cooperative tool-call budget.
 */
export function applyBrowserScreenshotTool(ctx: Context, session: SharedBrowserSession, timeoutMs: number): void {
  ctx.tools.register(defineTool({
    name: 'browser_screenshot',
    description: 'Capture a PNG screenshot of the browser\'s current page and save it to a temp file; returns the file path and size.',
    parameters: {
      full_page: { type: 'boolean', description: 'Capture the full scrollable page instead of the viewport.' },
    },
    output: {
      schema: {
        type: 'object',
        additionalProperties: false,
        properties: {
          path: { type: 'string', required: true },
          bytes: { type: 'integer', required: true },
        },
      },
      render: (_args, value) => [{ type: 'text' as const, text: formatScreenshotOutput(value.path, value.bytes) }],
    },
    timeoutMs,
    isConcurrencySafe: () => false,
    async execute(args, exec) {
      const shot = await session.run(browser => browser.screenshot({ ...(args.full_page === true ? { fullPage: true } : {}) }, exec.signal))
      screenshotCounter += 1
      const path = join(tmpdir(), `dsh-tool-browser-${Date.now()}-${screenshotCounter}.png`)
      await writeFile(path, shot.data)
      return { path, bytes: shot.data.byteLength }
    },
  }))
}
