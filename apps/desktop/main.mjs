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
import { join } from 'node:path'
import { app, BrowserWindow } from 'electron'

const TARGET_URL = process.env.DSH_DESKTOP_URL ?? 'http://127.0.0.1:3080'
const LOAD_BUDGET_MS = 30_000
const RETRY_DELAY_MS = 400

/** Load the target with bounded retries; resolves when the page commits. */
async function loadWithRetry(window) {
  const started = Date.now()
  for (;;) {
    try {
      await window.loadURL(TARGET_URL)
      return
    } catch (error) {
      if (Date.now() - started > LOAD_BUDGET_MS) {
        await window.loadURL(
          'data:text/html;charset=utf-8,' + encodeURIComponent(
             `<body style="font-family:system-ui;padding:3rem;background:#16181c;color:#e8eaed">
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
  const window = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 960,
    minHeight: 600,
    autoHideMenuBar: true,
    backgroundColor: '#16181c',
    title: 'DeepSeek Harness',
    show: false,
    useContentSize: true,
    webPreferences: {
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
    process.stdout.write(`[desktop] loaded ${window.webContents.getURL()} title=${JSON.stringify(window.getTitle())}\n`)
  })
  // Real-time fault surface: everything the renderer logs, every failed
  // navigation, and any renderer crash lands in this process's stdout so the
  // dev launcher log is the single place to watch.
  window.webContents.on('console-message', (_event, level, message, line, sourceId) => {
    if (level >= 2) process.stdout.write(`[renderer:${level}] ${message} (${sourceId}:${line})\n`)
  })
  window.webContents.on('did-fail-load', (_event, code, description, url) => {
    // Retry storms would flood the log; surface the first failure only, and
    // re-arm after any successful load.
    if (code === -3 || loadFailures > 0) return // ABORTED: an in-flight retry superseded this.
    loadFailures += 1
    process.stdout.write(`[desktop] did-fail-load code=${code} ${description} url=${url}\n`)
  })
  window.webContents.on('render-process-gone', (_event, details) => {
    process.stdout.write(`[desktop] render-process-gone ${JSON.stringify(details)}\n`)
  })
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
