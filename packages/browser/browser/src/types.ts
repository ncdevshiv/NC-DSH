/**
 * Vocabulary for the browser-automation capability seam (`ctx.browser`). The seam owns provider
 * registration, selection, cancellation, and errors; one launched {@link BrowserSession} is a
 * stateful, sequential interaction surface (cookies, history, current page) that a consumer owns
 * for its lifetime. Requests deliberately omit presentation and timeout controls: cancellation is
 * a direct execution argument, rendered output belongs to the consumer, and tool-call budgets
 * belong to the guard policy.
 * @module @deepseek-ai/dsh-browser/types
 */

import { HarnessError } from '@deepseek-ai/dsh-llm'

/** What a session is asked to navigate to. */
export interface BrowserNavigateRequest {
  /** Absolute `http:` or `https:` URL; providers reject other schemes. */
  readonly url: string
}

/**
 * What a session is asked to click. `selector` is a CSS selector resolved against the current
 * page; the first match is clicked. Ref-based element handles are deferred until a snapshot
 * vocabulary exists.
 */
export interface BrowserClickRequest {
  /** CSS selector of the element to click. */
  readonly selector: string
}

/**
 * What a session is asked to type. Text goes into the first match of `selector` after clearing
 * it; when `submit` is true, `Enter` follows the text.
 */
export interface BrowserTypeRequest {
  /** CSS selector of the input element to fill. */
  readonly selector: string
  /** Text to enter after clearing the element's existing value. */
  readonly text: string
  /** Press `Enter` after typing. */
  readonly submit?: boolean
}

/** What a screenshot is asked to capture. */
export interface BrowserScreenshotRequest {
  /** Capture the full scrollable page instead of the viewport. */
  readonly fullPage?: boolean
}

/** A captured screenshot: PNG bytes with an explicit media type. */
export interface BrowserScreenshot {
  readonly mediaType: 'image/png'
  readonly data: Uint8Array
}

/**
 * Normalized page state after one interaction. `content` is the page's rendered textual content
 * in the provider's model-friendly form (markdown or semantic text); providers bound its length.
 * `title` and `content` stay optional because not every interaction yields readable text
 * (downloads, empty tabs).
 */
export interface BrowserPageState {
  /** Final URL of the current page after the interaction. */
  readonly url: string
  readonly title?: string
  readonly content?: string
}

/**
 * One launched browser session. A session is stateful and sequential: navigation, cookies, and
 * storage persist across calls on the same instance. Consumers issue calls one at a time;
 * implementations serialize internally. Every method honors `signal` for cancellation, and
 * `close()` is idempotent — a consumer may close from a disposal path that races an in-flight call.
 */
export interface BrowserSession {
  /** Navigate to a URL and wait for load; resolves with the loaded page's state. */
  navigate(request: BrowserNavigateRequest, signal?: AbortSignal): Promise<BrowserPageState>
  /** Read the current page without navigating. */
  snapshot(signal?: AbortSignal): Promise<BrowserPageState>
  /** Click the first match of a CSS selector; resolves with the page state after the click. */
  click(request: BrowserClickRequest, signal?: AbortSignal): Promise<BrowserPageState>
  /** Fill the first match of a CSS selector; resolves with the page state after typing. */
  type(request: BrowserTypeRequest, signal?: AbortSignal): Promise<BrowserPageState>
  /** Capture a PNG of the current page. */
  screenshot(request: BrowserScreenshotRequest, signal?: AbortSignal): Promise<BrowserScreenshot>
  /** Release the session and its underlying resources; idempotent. */
  close(): Promise<void>
}

/**
 * A browser-automation backend. Registered with `ctx.browser.registerProvider`. `id` is stable
 * and unique within the registry. A backend may be multi-tenant over one process (for example a
 * long-lived automation server) or launch per session; consumers only see sessions.
 */
export interface BrowserProvider {
  readonly id: string
  /**
   * Cheap local usability check. It may probe a local binary's presence once,
   * but it never opens network connections and never starts a long-lived
   * process — session launch is {@link launch}'s job alone.
   */
  available(): boolean
  /**
   * Start or attach to a browser session. Implementations own the lifecycle controller for the
   * underlying process/connection; `signal` aborts startup, and the returned session must remain
   * closeable afterwards.
   */
  launch(signal?: AbortSignal): Promise<BrowserSession>
}

/**
 * Typed browser error with a machine-routable, open-string `code` and chained `cause`. Shared
 * codes cover unavailable, missing, unusable, ambiguous, or duplicate providers and cancellation,
 * mirroring the web seam; browser-specific codes cover invalid navigation targets, missing
 * elements, navigation failures, capture failures, oversized captures, and failed in-page
 * evaluation. Tool execution exposes the code in structured error metadata.
 */
export class BrowserError extends HarnessError {}
