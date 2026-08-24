/**
 * Electron main process for the DSH desktop shell. The window is a native
 * chrome over the web profile's loopback server: all business state, the
 * session runtime host plane, and the client-plugin module table stay exactly
 * where the browser story has them — behind `dsh web` on 127.0.0.1. This
 * process owns no backend; it renders whatever DSH_DESKTOP_URL (or the
 * composed default) serves and exits when the last window closes.
 *
 * Load failures retry briefly so a slow host boot does not surface as a dead
 * white window; after the budget the error page explains the likely cause.
 *
 * A second loopback listener serves debug window captures (see README):
 * any local process — including the dsh agent itself — screenshots a running
 * window through it instead of shelling out to OS capture tools.
 */
import { createServer } from 'node:http'
import { randomBytes } from 'node:crypto'
import { mkdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { app, BrowserWindow, ipcMain, nativeTheme } from 'electron'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const PRELOAD_PATH = join(__dirname, 'preload.cjs')
// Height of the native caption-button band and of the app's own title-bar row
// (sidebar brand row / session header), so both center their content on the
// same line. The renderer's chrome (ui-layout) owns the matching CSS height.
const TITLE_BAR_HEIGHT = 40

/** Normalize a CSS color string to a hex string Electron accepts (#rrggbb). */
function toHexColor(value) {
  if (typeof value !== 'string') return null
  const trimmed = value.trim()
  if (/^#[0-9a-fA-F]{6}$/.test(trimmed)) return trimmed.toLowerCase()
  if (/^#[0-9a-fA-F]{3}$/.test(trimmed)) {
    return `#${trimmed[1]}${trimmed[1]}${trimmed[2]}${trimmed[2]}${trimmed[3]}${trimmed[3]}`.toLowerCase()
  }
  const rgb = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/.exec(trimmed)
  if (rgb !== null) {
    const r = Math.max(0, Math.min(255, Number(rgb[1]))).toString(16).padStart(2, '0')
    const g = Math.max(0, Math.min(255, Number(rgb[2]))).toString(16).padStart(2, '0')
    const b = Math.max(0, Math.min(255, Number(rgb[3]))).toString(16).padStart(2, '0')
    return `#${r}${g}${b}`
  }
  return null
}

/** Apply theme colors to the native window chrome. */
function applyWindowTheme(window, bg, symbolColor) {
  const bgHex = toHexColor(bg)
  const symbolHex = toHexColor(symbolColor)
  if (bgHex !== null) {
    try { window.setBackgroundColor(bgHex) } catch {}
    try {
      window.setTitleBarOverlay({
        color: bgHex,
        symbolColor: symbolHex ?? (bgHex === '#ffffff' || bgHex === '#fff' ? '#0f1115' : '#f9fafb'),
        height: TITLE_BAR_HEIGHT,
      })
    } catch {}
  } else if (symbolHex !== null) {
    try {
      window.setTitleBarOverlay({ color: '#ffffff', symbolColor: symbolHex, height: TITLE_BAR_HEIGHT })
    } catch {}
  }
}

ipcMain.on('dsh:set-theme', (event, payload) => {
  const window = BrowserWindow.fromWebContents(event.sender)
  if (window === null || window.isDestroyed()) return
  const bg = payload?.bg
  const symbolColor = payload?.symbolColor
  applyWindowTheme(window, bg, symbolColor)
})

const TARGET_URL = process.env.DSH_DESKTOP_URL ?? 'http://127.0.0.1:3080'
const LOAD_BUDGET_MS = 30_000
const RETRY_DELAY_MS = 400
// Post-commit recovery budgets (main-process supervision; see createWindow).
const LOAD_RETRY_MAX = 8
const GONE_RELOAD_MAX = 8
const BLANK_SAMPLE_INTERVAL_MS = 10_000
const BLANK_RELOAD_MAX = 3
const BLANK_BUDGET_WINDOW_MS = 10 * 60_000

/** Load the target with bounded retries; resolves when the page commits. */
async function loadWithRetry(window) {
  const started = Date.now()
  for (;;) {
    try {
      await window.loadURL(TARGET_URL)
      return
    } catch (error) {
      if (Date.now() - started > LOAD_BUDGET_MS) {
        let bg = '#ffffff'
        let fg = '#0f1115'
        try {
          const current = window.getBackgroundColor()
          const normalized = toHexColor(current) ?? current
          if (normalized.toLowerCase() === '#151517' || normalized.toLowerCase() === '#16181c') {
            bg = '#151517'
            fg = '#e8eaed'
          }
        } catch {
          if (nativeTheme.shouldUseDarkColors) {
            bg = '#151517'
            fg = '#e8eaed'
          }
        }
        await window.loadURL(
          'data:text/html;charset=utf-8,' + encodeURIComponent(
             `<body style="font-family:system-ui;padding:3rem;background:${bg};color:${fg}">
              <h2>DeepSeek Harness desktop could not reach ${TARGET_URL}</h2>
              <p>Start the backend first (<code>bun dsh web</code>) or launch via <code>bun run dev:desktop</code>.</p>
              <pre style="color:#f66">${String(error)}</pre></body>`),
        )
        return
      }
      await new Promise((resolve) => setTimeout(resolve, RETRY_DELAY_MS))
    }
  }
}

function createWindow() {
  let loadFailures = 0
  let goneCount = 0
  const initialDark = nativeTheme.shouldUseDarkColors
  const initialBg = initialDark ? '#151517' : '#ffffff'
  const initialSymbol = initialDark ? '#f9fafb' : '#0f1115'
  const window = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 960,
    minHeight: 600,
    autoHideMenuBar: true,
    backgroundColor: initialBg,
    titleBarStyle: 'hidden',
    titleBarOverlay: {
      color: initialBg,
      symbolColor: initialSymbol,
      height: TITLE_BAR_HEIGHT,
    },
    title: 'DeepSeek Harness',
    show: false,
    useContentSize: true,
    webPreferences: {
      preload: PRELOAD_PATH,
      contextIsolation: true,
      nodeIntegration: false,
      zoomFactor: 1,
      enableBlinkFeatures: '',
    },
  })
  // Neutralize any previous zoom / DSF mismatch that produced the 25% top-left
  // letterbox on HiDPI Windows (devicePixelRatio 2 => 0.5*0.5 area). The CSS
  // fix in base.css keeps the layout filling the viewport; this keeps the
  // Chromium zoom at a deterministic 1 regardless of OS scale.
  try { window.webContents.setZoomFactor(1) } catch {}
  window.once('ready-to-show', () => {
    try { window.webContents.setZoomFactor(1) } catch {}
    window.show()
  })
  window.webContents.on('did-finish-load', () => {
    loadFailures = 0
    goneCount = 0
    process.stdout.write(`[desktop] loaded ${window.webContents.getURL()} title=${JSON.stringify(window.getTitle())}\n`)
    // Theme race: the boot HTML already sets the resolved light/dark palette
    // before client plugins activate. Sync once from the computed style so the
    // native title bar matches even before ThemePresenter pushes via IPC.
    window.webContents.executeJavaScript(`
      (() => {
        try {
          const bg = getComputedStyle(document.body).backgroundColor
          const dark = document.body.hasAttribute('data-ds-dark-theme')
          return { bg, dark }
        } catch { return null }
      })()
    `, true).then((result) => {
      if (result?.bg) {
        const symbol = result.dark ? '#f9fafb' : '#0f1115'
        applyWindowTheme(window, result.bg, symbol)
      }
    }).catch(() => {})
  })
  // Real-time fault surface: everything the renderer logs, every failed
  // navigation, and any renderer crash lands in this process's stdout so the
  // dev launcher log is the single place to watch.
  window.webContents.on('console-message', (_event, level, message, line, sourceId) => {
    if (level >= 2) process.stdout.write(`[renderer:${level}] ${message} (${sourceId}:${line})\n`)
  })
  window.webContents.on('did-fail-load', (_event, code, description, url, isMainFrame) => {
    if (!isMainFrame) return // subframe failures don't warrant reloading the app
    if (code === -3) return // ABORTED: an in-flight retry superseded this.
    // Log the first failure of a burst only; every failure still schedules the
    // next attempt, otherwise one failed retry would swallow the whole chain
    // (the early return below used to do exactly that).
    const firstOfBurst = loadFailures === 0
    loadFailures += 1
    if (firstOfBurst) {
      process.stdout.write(`[desktop] did-fail-load code=${code} ${description} url=${url}\n`)
    }
    // Recovery lives HERE in the main process, deliberately: a renderer whose
    // navigation failed cannot heal itself, so one failed load used to mean a
    // permanent background-colored window. Bounded exponential retry.
    if (loadFailures <= LOAD_RETRY_MAX && String(url).startsWith('http')) {
      const delay = Math.min(15_000, 1_000 * 2 ** (loadFailures - 1))
      setTimeout(() => { void window.loadURL(TARGET_URL).catch(() => {}) }, delay)
    }
  })
  window.webContents.on('render-process-gone', (_event, details) => {
    process.stdout.write(`[desktop] render-process-gone ${JSON.stringify(details)}\n`)
    if (goneCount >= GONE_RELOAD_MAX) return
    goneCount += 1
    setTimeout(() => { if (!window.isDestroyed()) window.webContents.reload() }, 1_000)
  })

  // Blank-pixel watchdog: samples the rendered frame on an interval and
  // reloads after three consecutive uniformly-colored frames. Pixel-based on
  // purpose — it needs zero cooperation from renderer code, so it still fires
  // when the page loaded but its JS died silently (the white/black-screen
  // classes). Budgeted like every supervisor here: at most BLANK_RELOAD_MAX
  // automatic reloads per BLANK_BUDGET_WINDOW_MS, then it logs once and stops
  // instead of masking a real failure forever.
  let consecutiveBlanks = 0
  let blankReloads = 0
  let blankReloadAt = 0
  const blankWatchdog = setInterval(() => {
    // A minimized or fully occluded window captures empty frames that look
    // exactly like a dead renderer; sampling them would burn the reload
    // budget on a healthy app the user simply hid.
    if (window.isDestroyed() || window.isMinimized() || !window.isVisible()) return
    if (window.webContents.isLoading()) return
    if (!window.webContents.getURL().startsWith('http')) return // error splashes are legitimately static
    void window.webContents.capturePage().then((image) => {
      if (image.isEmpty()) { consecutiveBlanks += 1 } else {
        // Grid sampling: a rendered shell always varies; a dead one is flat.
        const bitmap = image.getBitmap()
        const stride = Math.max(64, Math.floor(bitmap.length / 1024)) * 4
        let uniform = bitmap.length > 3
        let first = -1
        for (let offset = 0; offset + 2 < bitmap.length; offset += stride) {
          const value = bitmap[offset] + bitmap[offset + 1] * 256 + bitmap[offset + 2] * 65536
          if (first < 0) { first = value; continue }
          if (Math.abs(value - first) > 300_000) { uniform = false; break }
        }
        consecutiveBlanks = uniform ? consecutiveBlanks + 1 : 0
      }
      if (consecutiveBlanks < 3) return
      consecutiveBlanks = 0
      if (Date.now() - blankReloadAt > BLANK_BUDGET_WINDOW_MS) blankReloads = 0
      if (blankReloads >= BLANK_RELOAD_MAX) {
        if (blankReloads === BLANK_RELOAD_MAX) {
          blankReloads += 1
          process.stdout.write('[desktop] blank-frame watchdog exhausted its budget; leaving the window alone\n')
        }
        return
      }
      blankReloads += 1
      blankReloadAt = Date.now()
      process.stdout.write(`[desktop] blank frame x3; forcing reload (${String(blankReloads)}/${String(BLANK_RELOAD_MAX)} of budget)\n`)
      window.webContents.reload()
    }).catch(() => {})
  }, BLANK_SAMPLE_INTERVAL_MS)
  window.once('closed', () => clearInterval(blankWatchdog))
  void loadWithRetry(window)
}

// ---------------------------------------------------------------------------
// Debug window capture
//
// GET  /debug/windows.json                 every window: id, title, URL, bounds
// GET  /debug/screenshot.png[?window=<id>] PNG of that window's page content
//
// Both routes require the per-run `token` query parameter published only in
// the discovery file under DEBUG_DIR; possession of that file is the capture
// grant. Bind stays on 127.0.0.1, no CORS headers are sent (cross-origin page
// reads fail the same-origin check), and DSH_DESKTOP_DEBUG_CAPTURE=0 opts out.
// ---------------------------------------------------------------------------

const DEBUG_DIR = join(tmpdir(), 'dsh-desktop-debug')
const DEBUG_DISABLED = process.env.DSH_DESKTOP_DEBUG_CAPTURE === '0'

/**
 * Resolve the window a request names: the explicit id, else the focused
 * window, else the first window.
 */
function selectCaptureTarget(idParam) {
  const windows = BrowserWindow.getAllWindows()
  if (idParam === null) return BrowserWindow.getFocusedWindow() ?? windows[0] ?? null
  return windows.find((window) => window.id === Number(idParam)) ?? null
}

/**
 * PNG bytes of one window's page content, independent of what overlaps it on
 * screen. A minimized window has no live compositor surface: restore it for
 * the capture and put it back afterwards.
 */
async function captureWindowPng(window) {
  const wasMinimized = window.isMinimized()
  if (wasMinimized) {
    window.restore()
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  try {
    const image = await window.webContents.capturePage()
    return image.isEmpty() ? null : image.toPNG()
  } finally {
    if (wasMinimized) window.minimize()
  }
}

function sendJson(response, status, payload) {
  response.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' })
  response.end(JSON.stringify(payload))
}

async function handleDebugRequest(request, response, token) {
  if (request.method !== 'GET') return sendJson(response, 405, { error: 'GET only' })
  const url = new URL(request.url ?? '/', 'http://127.0.0.1')
  if (url.searchParams.get('token') !== token) return sendJson(response, 403, { error: 'invalid token' })

  if (url.pathname === '/debug/windows.json') {
    return sendJson(response, 200, {
      pid: process.pid,
      targetUrl: TARGET_URL,
      windows: BrowserWindow.getAllWindows().map((window) => ({
        id: window.id,
        title: window.getTitle(),
        url: window.webContents.getURL(),
        minimized: window.isMinimized(),
        bounds: window.getBounds(),
      })),
    })
  }

  if (url.pathname === '/debug/screenshot.png') {
    const window = selectCaptureTarget(url.searchParams.get('window'))
    if (window === null) return sendJson(response, 404, { error: 'no such window' })
    const png = await captureWindowPng(window)
    if (png === null) return sendJson(response, 409, { error: 'window has not painted any content yet' })
    response.writeHead(200, { 'content-type': 'image/png', 'cache-control': 'no-store' })
    return response.end(png)
  }

  sendJson(response, 404, { error: 'unknown route' })
}

/**
 * Serve the loopback debug-capture endpoint and publish its discovery record;
 * a no-op when the caller disabled capture. The record is removed on quit so
 * dead shells never stay discoverable; a crashed shell leaves residue that
 * consumers must skip by pid liveness.
 */
function startDebugCapture() {
  if (DEBUG_DISABLED) return
  const token = randomBytes(16).toString('hex')
  const endpointFile = join(DEBUG_DIR, `endpoint-${process.pid}.json`)
  const server = createServer((request, response) => {
    handleDebugRequest(request, response, token).catch((error) => {
      sendJson(response, 500, { error: String(error) })
    })
  })
  server.listen(0, '127.0.0.1', () => {
    const port = /** @type {import('node:net').AddressInfo} */ (server.address()).port
    mkdirSync(DEBUG_DIR, { recursive: true })
    writeFileSync(endpointFile, JSON.stringify({ pid: process.pid, port, token }, null, 2) + '\n')
    // The discovery file carries the token; the log records only where it lives.
    process.stdout.write(`[desktop] debug capture on http://127.0.0.1:${port}/debug (discovery: ${endpointFile})\n`)
  })
  app.once('will-quit', () => {
    rmSync(endpointFile, { force: true })
    server.close()
  })
}

app.whenReady().then(() => {
  createWindow()
  startDebugCapture()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  app.quit()
})
