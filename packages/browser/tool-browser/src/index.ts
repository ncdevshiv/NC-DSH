/**
 * Model-facing browser tools over `ctx.browser`: navigate, snapshot, click,
 * type, and screenshot through one shared, serialized session. This package
 * owns schemas, validation, prompt guidance, budgets, and output rendering —
 * never concrete providers. Enablement controls registration; an enabled tool
 * stays visible when no provider is usable and fails with a structured
 * `BrowserError` at execution time.
 * @module @deepseek-ai/dsh-tool-browser
 */

import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {} from '@deepseek-ai/dsh-browser'
import type {} from '@deepseek-ai/dsh-tools'
import type {} from '@deepseek-ai/dsh-system-prompt'
import { SharedBrowserSession } from './session-holder.ts'
import {
  applyBrowserClickTool,
  applyBrowserNavigateTool,
  applyBrowserScreenshotTool,
  applyBrowserSnapshotTool,
  applyBrowserTypeTool,
} from './tools.ts'

export { SharedBrowserSession } from './session-holder.ts'
export {
  applyBrowserClickTool,
  applyBrowserNavigateTool,
  applyBrowserScreenshotTool,
  applyBrowserSnapshotTool,
  applyBrowserTypeTool,
  type PageToolValue,
  type ScreenshotToolValue,
} from './tools.ts'
export { assertNonBlank, formatPageState, formatScreenshotOutput } from './output.ts'

/** Cordis plugin name used by loader diagnostics. */
export const name = 'tool-browser'

/** Services required by the browser tool suite. */
export const inject = ['tools', 'browser', 'systemPrompt']

/** Default cooperative tool-call budget (ms) for navigation. */
export const DEFAULT_NAVIGATION_TIMEOUT_MS = 30_000

/** Default cooperative tool-call budget (ms) for in-page actions and captures. */
export const DEFAULT_ACTION_TIMEOUT_MS = 15_000

/** Default cap on one browser tool's rendered output characters. */
export const DEFAULT_MAX_OUTPUT_CHARS = 20_000

/** Default cap on one screenshot's encoded PNG size in bytes. */
export const DEFAULT_MAX_SCREENSHOT_BYTES = 5_242_880

/**
 * The shared system-prompt guidance for every enabled browser tool. One
 * section regardless of which tools are enabled, so toggling a single action
 * does not rewrite guidance text.
 */
const BROWSER_GUIDANCE = 'Use the browser_* tools to drive a real headless browser: browser_navigate loads a URL, browser_snapshot reads the current page state, browser_click and browser_type act on elements matched by CSS selector (use browser_snapshot first to find selectors), and browser_screenshot saves a PNG of the current page and returns its path. The session persists across calls within a conversation.'

/** Plugin config: which browser tools to register and their budgets. */
export interface Config {
  /** Register `browser_navigate`. Defaults to true. */
  navigate?: boolean
  /** Register `browser_snapshot`. Defaults to true. */
  snapshot?: boolean
  /** Register `browser_click`. Defaults to true. */
  click?: boolean
  /** Register `browser_type`. Defaults to true. */
  typing?: boolean
  /** Register `browser_screenshot`. Defaults to true. */
  screenshot?: boolean
  /** Cooperative budget (ms) for `browser_navigate`. Defaults to 30000. */
  navigationTimeoutMs?: number
  /** Cooperative budget (ms) for in-page actions and screenshots. Defaults to 15000. */
  actionTimeoutMs?: number
  /** Cap on one tool's complete rendered output characters. Defaults to 20000. */
  maxOutputChars?: number
  /** Cap on one screenshot's encoded PNG size in bytes. Defaults to 5242880 (5 MiB). */
  maxScreenshotBytes?: number
}

export const Config: z<Config> = z.object({
  navigate: z.boolean().default(true),
  snapshot: z.boolean().default(true),
  click: z.boolean().default(true),
  typing: z.boolean().default(true),
  screenshot: z.boolean().default(true),
  navigationTimeoutMs: z.number().default(DEFAULT_NAVIGATION_TIMEOUT_MS),
  actionTimeoutMs: z.number().default(DEFAULT_ACTION_TIMEOUT_MS),
  maxOutputChars: z.number().default(DEFAULT_MAX_OUTPUT_CHARS),
  maxScreenshotBytes: z.number().default(DEFAULT_MAX_SCREENSHOT_BYTES),
})

/** Complete config after schemastery applies every field default. */
type ResolvedConfig = Required<Config>

/** Configured timeout and character caps must be positive integers. */
function assertPositiveInteger(name: string, value: number): void {
  if (!Number.isInteger(value) || value < 1) {
    throw new Error(`tool-browser: ${name} must be a positive integer`)
  }
}

/**
 * Register the enabled browser tools over one lazily-launched session. The
 * session's disposal is fiber-scoped via `ctx.effect`, so HMR or plugin
 * disposal closes the underlying browser exactly once.
 */
export function apply(ctx: Context, config: Config): void {
  // schemastery (Config) has already filled every defaulted field.
  const resolved = config as ResolvedConfig
  assertPositiveInteger('navigationTimeoutMs', resolved.navigationTimeoutMs)
  assertPositiveInteger('actionTimeoutMs', resolved.actionTimeoutMs)
  assertPositiveInteger('maxOutputChars', resolved.maxOutputChars)
  assertPositiveInteger('maxScreenshotBytes', resolved.maxScreenshotBytes)

  const anyEnabled = resolved.navigate || resolved.snapshot || resolved.click || resolved.typing || resolved.screenshot
  if (!anyEnabled) return

  const session = new SharedBrowserSession(ctx.browser)
  void ctx.effect(function* () {
    yield () => session.dispose()
  }, 'tool-browser.shared-session')

  ctx.systemPrompt.section({
    name: 'tool:browser',
    order: 112,
    text: BROWSER_GUIDANCE,
  })

  if (resolved.navigate) applyBrowserNavigateTool(ctx, session, resolved.navigationTimeoutMs, resolved.maxOutputChars)
  if (resolved.snapshot) applyBrowserSnapshotTool(ctx, session, resolved.actionTimeoutMs, resolved.maxOutputChars)
  if (resolved.click) applyBrowserClickTool(ctx, session, resolved.actionTimeoutMs, resolved.maxOutputChars)
  if (resolved.typing) applyBrowserTypeTool(ctx, session, resolved.actionTimeoutMs, resolved.maxOutputChars)
  if (resolved.screenshot) applyBrowserScreenshotTool(ctx, session, resolved.actionTimeoutMs, resolved.maxScreenshotBytes)
}
