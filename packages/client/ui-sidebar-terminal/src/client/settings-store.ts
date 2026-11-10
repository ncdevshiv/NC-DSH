/**
 * Terminal typography store: mirrors of the settings form snapshot.
 *
 * The plugin's apply-world change listener is the only writer; the row
 * components and the xterm instances read via the store, so a preference
 * changed in one terminal's session reaches every terminal it governs.
 */
import { defineStore, type EngineStoreHandle } from '@deepseek-ai/dsh-client-store'
import { DEFAULT_TERMINAL_FONT_FAMILY, DEFAULT_TERMINAL_FONT_SIZE } from '../terminal-settings.ts'
import type { TerminalFontFamily } from '../terminal-settings.ts'

/** Store state mirrored from the settings snapshot. */
export interface TerminalTypographyState {
  /** Persisted terminal font family stack. */
  fontFamily: TerminalFontFamily
  /** Persisted terminal font size in px. */
  fontSize: number
  /** Snapshot revision; -1 until first sync so revision 0 lands as a change. */
  revision: number
}

/** Declared action shape giving the exported factory a stable return type. */
type TerminalTypographyActions = {
  sync: (draft: TerminalTypographyState, fontFamily: TerminalFontFamily, fontSize: number, revision: number) => void
}

/**
 * Declares the terminal typography state and write surface.
 * @returns the store handle.
 */
export function createTerminalTypographyStore(): EngineStoreHandle<TerminalTypographyState, TerminalTypographyActions> {
  return defineStore({
    init: (): TerminalTypographyState => ({
      fontFamily: DEFAULT_TERMINAL_FONT_FAMILY,
      fontSize: DEFAULT_TERMINAL_FONT_SIZE,
      revision: -1,
    }),
    actions: {
      sync: (d, fontFamily, fontSize, revision) => {
        if (d.fontFamily === fontFamily && d.fontSize === fontSize && revision !== 0) return
        if (revision <= d.revision) return
        d.fontFamily = fontFamily
        d.fontSize = fontSize
        d.revision = revision
      },
    },
  })
}
