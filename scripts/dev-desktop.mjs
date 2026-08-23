/**
 * Desktop dev launcher: boots the pieces the Electron window renders and
 * tears them all down on exit.
 *
 * - `dsh web --no-open` (tsx source launch) serves the host plane, the API
 *   gateway, and the frontend artifacts on a loopback port. `--port 0` lets
 *   the OS pick a free port; the URL line the host prints is how this script
 *   learns it.
 * - `--hmr` additionally runs scripts/dev-web.ts, the watch-build that
 *   rewrites lib/client.js and apps/web/dist on source edits; the host
 *   stat-polls those artifacts itself and broadcasts `rebuilt` frames to
 *   connected renderers, which is the client-plugin HMR path — no Electron
 *   restart involved.
 * - The Electron main process (apps/desktop) loads the served URL once the
 *   host answers HTTP. The binary resolves through apps/desktop's own
 *   dependency, not PATH.
 *
 * Child teardown walks the process tree (`taskkill /T` on win32, negative-PID
 * group kill elsewhere) because dsh web spawns its own subtree.
 */
import { spawn } from 'node:child_process'
import { createRequire } from 'node:module'
import { platform } from 'node:os'
import { mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = fileURLToPath(new URL('..', import.meta.url))
const withHmr = process.argv.includes('--hmr')
const desktopRequire = createRequire(fileURLToPath(new URL('../apps/desktop/package.json', import.meta.url)))
// The electron package's main export IS the platform binary path; resolving it
// here works from any invocation context (`bun run` or bare `node`).
const electronBinary = process.env.ELECTRON_COMMAND ?? desktopRequire('electron')

/** The backend URL line, captured as children stream output. */
let urlLine

/**
 * One piped child with repo-root cwd.
 * @returns the child plus an exit promise, so teardown can wait for settle.
 */
function start(command, args, label, extraEnv = {}) {
  const child = spawn(command, args, {
    cwd: repoRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, ...extraEnv },
  })
  const pipe = (stream) => {
    stream.setEncoding('utf8')
    let buffer = ''
    stream.on('data', (chunk) => {
      buffer += chunk
      for (;;) {
        const at = buffer.indexOf('\n')
        if (at < 0) break
        const line = buffer.slice(0, at)
        buffer = buffer.slice(at + 1)
        process.stdout.write(`[${label}] ${line}\n`)
        if (line.includes('http://')) urlLine = line
      }
    })
  }
  pipe(child.stdout)
  pipe(child.stderr)
  const done = new Promise((resolve) => child.on('exit', (code, signal) => {
    // A dead child must be visible the moment it dies, not discovered later.
    process.stdout.write(`[${label}] exited code=${code} signal=${signal}\n`)
    resolve({ code, signal })
  }))
  return { child, done }
}

const children = []
// The desktop run gets an isolated DSH_HOME unless the caller pinned one: the
// web profile auto-initializes there, so a user home carrying foreign or
// unparseable state (credential files from other tooling) cannot block boot.
const desktopHome = process.env.DSH_HOME ?? join(tmpdir(), 'dsh-desktop-home')
mkdirSync(desktopHome, { recursive: true })
children.push(start(process.execPath, ['--import', 'tsx/esm', 'apps/cli/src/bin.ts', 'web', '--port', '0', '--no-open'], 'web', { DSH_HOME: desktopHome }))

if (withHmr) {
  children.push(start(process.execPath, ['--import', 'tsx/esm', 'scripts/dev-web.ts', '--poll'], 'watch'))
}

/** Kill the full child tree; win32 needs taskkill for grandchildren. */
function teardown() {
  for (const { child } of children) {
    if (child.pid === undefined || child.exitCode !== null) continue
    if (platform() === 'win32') {
      spawn('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
    } else {
      try {
        process.kill(-child.pid, 'SIGTERM')
      } catch {
        child.kill()
      }
    }
  }
}

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => {
    teardown()
    process.exit(130)
  })
}

// Wait for the host's printed URL line, then raise the window.
const deadline = Date.now() + 120_000
while ((urlLine === undefined || !urlLine.includes('http://')) && Date.now() < deadline) {
  await new Promise((resolve) => setTimeout(resolve, 250))
}
if (urlLine === undefined || !urlLine.includes('http://')) {
  console.error('[desktop] the web profile never printed a URL; not starting Electron')
  teardown()
  process.exit(1)
}
const url = /http:\/\/\S+/.exec(urlLine)?.[0] ?? 'http://127.0.0.1:3080'
process.stdout.write(`[desktop] backend ready at ${url}\n`)

children.push(start(electronBinary, ['apps/desktop'], 'electron', { DSH_DESKTOP_URL: url }))

const results = await Promise.all(children.map((entry) => entry.done))
teardown()
const failed = results.find((entry) => entry.code !== 0 && entry.signal === null)
process.exitCode = failed ? (failed.code ?? 1) : 0
