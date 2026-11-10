/**
 * Watch the Host source roots and signal a rebuild by rewriting a marker file.
 *
 * The desktop launcher polls that marker instead of relying on PowerShell
 * `FileSystemWatcher` event delivery, which is not reliable in a
 * `powershell -File` host: measured on this machine, a `Register-ObjectEvent`
 * action's side effect reached the polling loop in 1 of 3 runs with one watcher
 * and 0 of 3 with the launcher's fourteen.
 *
 * Usage: node scripts/watch-host.mjs <repo-root> [marker-path]
 *
 * The marker carries only a timestamp; the launcher owns the build and the app
 * lifecycle. Exclusions mirror the launcher's previous filters so a build
 * output or an install cannot look like a source edit.
 */
import { statSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { watch } from 'node:fs'

const root = resolve(process.argv[2] ?? process.cwd())
const marker = resolve(process.argv[3] ?? resolve(root, 'apps/desktop/.desktop-build/development/rebuild-requested'))
const targets = ['packages', 'apps/desktop/src', 'apps/web', 'vendor']
const excluded = /(^|[\\/])(node_modules|lib|dist|visualizations)([\\/]|$)|.*\.tsbuildinfo$/iu
const startedAt = Date.now()

let timer
const signal = () => {
  clearTimeout(timer)
  timer = setTimeout(() => {
    try {
      writeFileSync(marker, `${String(Date.now())}\n`, 'utf8')
    } catch (error) {
      process.stderr.write(`watch-host: cannot write marker: ${String(error)}\n`)
    }
  }, 50)
}

for (const target of targets) {
  try {
    const base = resolve(root, target)
    watch(base, { recursive: true }, (event, filename) => {
      if (filename === null || excluded.test(filename)) return
      try {
        // Node's recursive fs.watch on Windows reports the existing tree as
        // change events when the watch is established; those carry their
        // original mtime, so compare against this process's start time.
        if (statSync(resolve(base, filename)).mtimeMs < startedAt - 1000) return
      } catch {
        // A path that no longer stats is a deletion, which is a real change.
      }
      process.stderr.write(`watch-host: ${event} ${filename}\n`)
      signal()
    })
  } catch (error) {
    process.stderr.write(`watch-host: cannot watch ${target}: ${String(error)}\n`)
  }
}

process.on('SIGTERM', () => process.exit(0))
process.on('SIGINT', () => process.exit(0))
