/**
 * Argument validation and model-facing output formatting for the browser
 * tools — the pure half. Rendering is a pure function of the seam's results,
 * bounded by the deployment's character cap.
 * @module @deepseek-ai/dsh-tool-browser/output
 */

import type { BrowserPageState } from '@deepseek-ai/dsh-browser'

/** The truncation notice appended when the output cap cuts a page state. */
const TRUNCATION_FOOTER = '\n\n(Content truncated.)'

/**
 * Validate a non-blank string argument the schema DSL cannot constrain.
 * @param name - the argument name, quoted in the error.
 * @param value - the schema-validated string.
 * @returns the trimmed-back original value.
 */
export function assertNonBlank(name: string, value: string): string {
  if (value.trim().length === 0) throw new Error(`${name} must be a non-empty string`)
  return value
}

/**
 * Format one page state as the tool's complete model-facing text.
 *
 * @param verb - what the tool did (`Navigated to`, `Snapshot of`, `Clicked`, `Typed into`).
 * @param state - the seam's resulting page state.
 * @param maxOutputChars - cap on the complete returned text.
 * @returns the header line plus optional title and content, capped as a whole.
 */
export function formatPageState(verb: string, state: BrowserPageState, maxOutputChars: number): string {
  const prefix = `${verb} ${state.url}\n\n`
  const middle = state.title !== undefined ? `${state.title}\n\n` : ''
  const full = `${prefix}${middle}${state.content ?? ''}`
  if (full.length <= maxOutputChars) return full
  const body = full.slice(0, Math.max(0, maxOutputChars - TRUNCATION_FOOTER.length))
  return `${body}${TRUNCATION_FOOTER}`
}

/**
 * Format one screenshot outcome as the model-facing envelope. The PNG rides a
 * file path; inline pixel delivery waits on attachment-backed image blocks.
 *
 * @param path - absolute path of the saved PNG.
 * @param bytes - the encoded size in bytes.
 * @returns the envelope text naming the file and its size.
 */
export function formatScreenshotOutput(path: string, bytes: number): string {
  return `<path>${path}</path>\n<type>image/png</type>\n<content>\nPNG screenshot, ${bytes} bytes\n</content>`
}
