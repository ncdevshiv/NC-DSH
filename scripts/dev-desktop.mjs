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
import { createWriteStream, mkdirSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = fileURLToPath(new URL('..', import.meta.url))
const withHmr = process.argv.includes('--hmr')
const forceSecondInstance = process.argv.includes('--force')
// --replace: the previous stack is killed instead of refused, making
// "just relaunch it" always work. This is the zombie-stack cure: a launcher
// whose window died but whose node tree lingers otherwise holds the lock
// forever and leaves a dead renderer on screen.
const replaceExisting = process.argv.includes('--replace')

// ── single-instance guard ───────────────────────────────────────────────────
//
// Two desktop stacks must not run at once: both write the same `lib/` and
// `apps/web/dist/` trees (dev-web.ts's own docstring forbids exactly this for
// `bun run build`), and concurrent vite public-dir copies over one asset
// collide with EBUSY, killing a watcher and stranding a stale artifact chain.
const launchLockPath = join(tmpdir(), 'dsh-desktop-dev.lock')

/** @param pid - a pid that may belong to another live launcher. */
function pidAlive(pid) {
  try {
    process.kill(pid, 0)
    return true
  } catch (error) {
    return error.code === 'EPERM'
  }
}

try {
  const held = JSON.parse(readFileSync(launchLockPath, 'utf8'))
  if (held.pid !== process.pid && pidAlive(held.pid)) {
    if (replaceExisting) {
      console.error(`[desktop] --replace: killing previous stack (pid ${String(held.pid)}${typeof held.url === 'string' ? `, ${held.url}` : ''})`)
      if (platform() === 'win32') {
        spawn('taskkill', ['/pid', String(held.pid), '/T', '/F'], { stdio: 'ignore' })
      } else {
        try { process.kill(held.pid, 'SIGTERM') } catch { /* already gone; taking the lock over is correct */ }
      }
    } else if (!forceSecondInstance) {
      console.error(
        `[desktop] another dev:desktop stack is running (pid ${String(held.pid)}`
        + `${typeof held.url === 'string' ? `, ${held.url}` : ''}). Close it first, `
        + 'pass --force to start a second one anyway (risks dist-write collisions), '
        + 'or pass --replace to kill it and take over.',
      )
      process.exit(2)
    }
  }
} catch {
  // No lock file, an unreadable one, or a dead owner's stale lock: taking the
  // lock over is the correct outcome in all three cases.
}
writeFileSync(launchLockPath, JSON.stringify({ pid: process.pid, hmr: withHmr, startedAt: new Date().toISOString() }))
process.on('exit', () => {
  try {
    if (JSON.parse(readFileSync(launchLockPath, 'utf8')).pid === process.pid) unlinkSync(launchLockPath)
  } catch {
    // The lock is already gone or unreadable; either way there is nothing to clean.
  }
})
const desktopRequire = createRequire(fileURLToPath(new URL('../apps/desktop/package.json', import.meta.url)))
// The electron package's main export IS the platform binary path; resolving it
// here works from any invocation context (`bun run` or bare `node`).
const electronBinary = process.env.ELECTRON_COMMAND ?? desktopRequire('electron')

/** The backend URL line, captured as children stream output. */
let urlLine

/**
 * Per-child log file under `<desktopHome>/logs/`. The console pipe is lost the
 * moment the launcher window closes, which previously left host-side failures
 * (e.g. slow session creates) with zero trace after the fact.
 */
let logsDir
function childLogStream(label) {
  if (logsDir === undefined) {
    logsDir = join(desktopHome ?? join(tmpdir(), 'dsh-desktop-home'), 'logs')
    mkdirSync(logsDir, { recursive: true })
  }
  const stamp = new Date().toISOString().replace(/[:.]/g, '-')
  return createWriteStream(join(logsDir, `${label}-${stamp}.log`), { flags: 'a' })
}

/**
 * One piped child with repo-root cwd; stdout/stderr lines echo to the console
 * and append to the child's persistent log file.
 * @returns the child plus an exit promise, so teardown can wait for settle.
 */
function start(command, args, label, extraEnv = {}) {
  const child = spawn(command, args, {
    cwd: repoRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, ...extraEnv },
  })
  const file = childLogStream(label)
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
        file.write(`${line}\n`)
        if (line.includes('http://')) urlLine = line
      }
    })
  }
  pipe(child.stdout)
  pipe(child.stderr)
  const done = new Promise((resolve) => child.on('exit', (code, signal) => {
    // A dead child must be visible the moment it dies, not discovered later.
    process.stdout.write(`[${label}] exited code=${code} signal=${signal}\n`)
    file.end()
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
// Record the URL in the launch lock so a second launcher's refusal message
// can point at the running instance directly.
try {
  writeFileSync(launchLockPath, JSON.stringify({ pid: process.pid, hmr: withHmr, startedAt: new Date().toISOString(), url }))
} catch {
  // Lock refresh is best-effort; the pid already written is what the guard reads.
}

children.push(start(electronBinary, ['apps/desktop'], 'electron', { DSH_DESKTOP_URL: url }))

const results = await Promise.all(children.map((entry) => entry.done))
teardown()
const failed = results.find((entry) => entry.code !== 0 && entry.signal === null)
process.exitCode = failed ? (failed.code ?? 1) : 0
