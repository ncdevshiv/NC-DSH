/** Terminal typography preferences stored in the Host user-settings document. */

import z from '@deepseek-ai/schemastery'
import { Volatile } from '@deepseek-ai/cordis'

/** Settings namespace owning the terminal's typography. */
export const TERMINAL_SETTINGS_NAMESPACE = 'ui-sidebar-terminal'

/** Field carrying the terminal font family stack. */
export const TERMINAL_FONT_FAMILY_FIELD = 'fontFamily'
/** Field carrying the terminal font size in pixels. */
export const TERMINAL_FONT_SIZE_FIELD = 'fontSize'

/**
 * Font stacks the terminal offers. Only stacks every target platform renders
 * without fetching are listed, so the control cannot choose a face that
 * resolves to something else on the user's machine.
 */
export const TERMINAL_FONT_FAMILIES = [
  'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
  'monospace',
  'Menlo, monospace',
  'Consolas, monospace',
  '"Courier New", monospace',
] as const

/** Terminal font family persisted by the General row. */
export type TerminalFontFamily = typeof TERMINAL_FONT_FAMILIES[number]

/** Smallest accepted terminal font size (px). */
export const TERMINAL_FONT_SIZE_MIN = 8
/** Largest accepted terminal font size (px). */
export const TERMINAL_FONT_SIZE_MAX = 24

/** Terminal font size when the user-settings document has no override (px). */
export const DEFAULT_TERMINAL_FONT_SIZE = 13

/**
 * Terminal font family when the user-settings document has no override.
 *
 * This is the face the xterm instance opened with before the preference
 * existed, so an existing deployment renders identically until the user picks.
 */
export const DEFAULT_TERMINAL_FONT_FAMILY: TerminalFontFamily = TERMINAL_FONT_FAMILIES[0]

/** Durable terminal typography section shared by the Host schema and the browser scope. */
export interface TerminalSettings {
  /** xterm `fontFamily` stack. */
  fontFamily: TerminalFontFamily
  /** xterm `fontSize` in pixels (integer within {@link TERMINAL_FONT_SIZE_MIN}..{@link TERMINAL_FONT_SIZE_MAX}). */
  fontSize: number
}

/** Live terminal typography preferences, as the Loader delivers them. */
export interface Config {
  /** Terminal font family stack. */
  fontFamily: Volatile<TerminalFontFamily>
  /** Terminal font size in pixels. */
  fontSize: Volatile<number>
}

/** Durable terminal typography schema; also the wire envelope the browser scope validates against. */
export const Config = z.object({
  [TERMINAL_FONT_FAMILY_FIELD]: z
    .union(TERMINAL_FONT_FAMILIES as unknown as readonly TerminalFontFamily[])
    .default(DEFAULT_TERMINAL_FONT_FAMILY)
    .volatile(),
  [TERMINAL_FONT_SIZE_FIELD]: z
    .number()
    .step(1)
    .min(TERMINAL_FONT_SIZE_MIN)
    .max(TERMINAL_FONT_SIZE_MAX)
    .default(DEFAULT_TERMINAL_FONT_SIZE)
    .volatile(),
})

