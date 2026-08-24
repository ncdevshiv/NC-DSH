/**
 * Global theme DOM applier: projects the resolved ThemeSnapshot onto the
 * document — `html { color-scheme }` for native UA chrome (scrollbars, form
 * controls), `body[data-ds-dark-theme]` for the token palette, the active
 * theme's alias-token overrides as inline CSS variables on body, and one
 * presenter-owned `meta[name="theme-color"]` for surrounding browser UI. Pure
 * DOM writes, no React involvement; the presenter only ever retracts what it
 * wrote itself, so foreign attributes, metadata, and inline styles survive.
 */
import type { ThemeSnapshot } from '@deepseek-ai/dsh-client-ui-theme/client'

/** Body attribute selecting the dark base palette in the token stylesheets. */
export const DARK_ATTRIBUTE = 'data-ds-dark-theme'

/** Convert an rgb(...) string to #rrggbb for the desktop bridge; leaves hex untouched. */
function toHex(value: string): string | null {
  const trimmed = value.trim()
  if (/^#[0-9a-fA-F]{6}$/.test(trimmed)) return trimmed.toLowerCase()
  if (/^#[0-9a-fA-F]{3}$/.test(trimmed)) {
    return `#${trimmed[1]}${trimmed[1]}${trimmed[2]}${trimmed[2]}${trimmed[3]}${trimmed[3]}`.toLowerCase()
  }
  const rgb = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/.exec(trimmed)
  if (rgb === null) return null
  const r = Math.max(0, Math.min(255, Number(rgb[1]))).toString(16).padStart(2, '0')
  const g = Math.max(0, Math.min(255, Number(rgb[2]))).toString(16).padStart(2, '0')
  const b = Math.max(0, Math.min(255, Number(rgb[3]))).toString(16).padStart(2, '0')
  return `#${r}${g}${b}`
}

/** Applies theme snapshots to the document; one instance per plugin fiber. */
export class ThemePresenter {
  /** Token names this presenter wrote in the last apply (its retraction set). */
  private appliedTokens: string[] = []
  /** The single metadata node this presenter inserts and removes. */
  private readonly themeColorMeta: HTMLMetaElement

  /** Create the presenter-owned metadata node before the first snapshot arrives. */
  constructor() {
    this.themeColorMeta = document.createElement('meta')
    this.themeColorMeta.name = 'theme-color'
  }

  /**
   * Project a snapshot onto the document: set root `color-scheme` and the body
   * palette attribute from `active.colorScheme` (never the id — `system` is
   * resolved upstream), then replace the previously applied token variables
   * with `active.tokens`. Browser theme-color metadata follows the computed
   * body background after those writes, so the rendered palette remains the
   * color authority.
   * @param snapshot - resolved theme snapshot from ctx.theme.
   */
  apply(snapshot: ThemeSnapshot): void {
    const scheme = snapshot.active.colorScheme
    document.documentElement.style.colorScheme = scheme
    const body = document.body
    if (scheme === 'dark') body.setAttribute(DARK_ATTRIBUTE, '')
    else body.removeAttribute(DARK_ATTRIBUTE)
    for (const name of this.appliedTokens) body.style.removeProperty(name)
    this.appliedTokens = []
    for (const [name, value] of Object.entries(snapshot.active.tokens)) {
      body.style.setProperty(name, value)
      this.appliedTokens.push(name)
    }
    const bg = getComputedStyle(body).backgroundColor
    this.themeColorMeta.content = bg
    if (!this.themeColorMeta.isConnected) document.head.append(this.themeColorMeta)
    this.syncDesktopChrome(bg, scheme)
  }

  /**
   * Push the resolved colors to the Electron desktop shell when running
   * inside it. No-op in the browser; the shell feature-detects
   * `window.dshDesktop.setTheme` exposed by its preload.
   */
  private syncDesktopChrome(bg: string, scheme: 'light' | 'dark'): void {
    try {
      const api = (window as unknown as { dshDesktop?: { setTheme?: (bg: string, symbolColor: string) => void } }).dshDesktop
      if (api?.setTheme === undefined) return
      const symbolColor = scheme === 'dark' ? '#f9fafb' : '#0f1115'
      api.setTheme(toHex(bg) ?? bg, symbolColor)
    } catch {
      // No desktop shell or bridge unavailable — browser is the only chrome.
    }
  }

  /** Retract root color-scheme, the palette attribute, token variables, and the owned metadata node. */
  dispose(): void {
    document.documentElement.style.removeProperty('color-scheme')
    const body = document.body
    body.removeAttribute(DARK_ATTRIBUTE)
    for (const name of this.appliedTokens) body.style.removeProperty(name)
    this.appliedTokens = []
    this.themeColorMeta.remove()
  }
}
