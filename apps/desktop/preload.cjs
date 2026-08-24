/**
 * Preload for the DSH desktop shell: exposes a minimal theme-sync bridge
 * so the renderer (ui-layout's ThemePresenter) can push the resolved title-bar
 * colors to the native window chrome. The renderer feature-detects
 * `window.dshDesktop.setTheme` and falls back to no-op in the browser.
 *
 * Preload scripts run under the sandbox renderer, which only accepts
 * CommonJS — ESM `import` syntax fails to load, so this file stays `.cjs`
 * despite the package's `"type": "module"`.
 */
const { contextBridge, ipcRenderer } = require('electron')

contextBridge.exposeInMainWorld('dshDesktop', {
  /**
   * Notify the main process of the current theme's window chrome colors.
   * @param bg - CSS hex background color for the title bar (e.g. "#ffffff").
   * @param symbolColor - CSS hex color for window-control glyphs.
   */
  setTheme(bg, symbolColor) {
    ipcRenderer.send('dsh:set-theme', { bg, symbolColor })
  },
})
