/** Run serial browser owners before one bounded snapshot pool. */
import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')

/**
 * Resolve the shell-free spawn for one Vitest invocation under the package
 * manager that invoked this process. Standalone binaries such as bun execute
 * directly against the workspace-installed launcher (bun exec resolves PATH
 * entries only); node-backed managers launch their JavaScript entrypoint.
 */
function vitestInvocation(vitestArgs: string[]): { command: string; args: string[] } {
  const entrypoint = process.env.npm_execpath
  if (entrypoint === undefined || entrypoint === '') {
    throw new Error('parallel web snapshots must be invoked through a bun package script.')
  }
  const userAgent = process.env.npm_config_user_agent ?? ''
  if (!(/.exe$/i.test(entrypoint) || userAgent.startsWith('bun/'))) {
    return { command: process.execPath, args: [entrypoint, 'exec', 'vitest', ...vitestArgs] }
  }
  const launcher = join(root, 'node_modules', '.bin', process.platform === 'win32' ? 'vitest.exe' : 'vitest')
  if (!existsSync(launcher)) throw new Error(`parallel web snapshots: vitest is not installed at ${launcher}.`)
  return { command: launcher, args: vitestArgs }
}

const serialFiles = [
  'apps/web/tests/hmr-live.e2e.ts',
  'apps/web/tests/cordis-tool-round.e2e.ts',
]
const workerRaw = process.env.DSH_WEB_SNAPSHOT_WORKERS
const workers = Number.parseInt(workerRaw ?? '', 10)
if (!Number.isSafeInteger(workers) || workers < 2 || String(workers) !== workerRaw) {
  throw new Error(`DSH_WEB_SNAPSHOT_WORKERS must be an integer greater than 1, got ${JSON.stringify(workerRaw)}.`)
}
const baseArgs = ['run', '--config', 'vitest.web.config.ts']
let serialStatus = 0
for (const file of serialFiles) {
  serialStatus = await run(vitestInvocation([...baseArgs, file]))
  if (serialStatus !== 0) break
}
if (serialStatus === 0) {
  process.exitCode = await run(vitestInvocation([
    ...baseArgs,
    ...serialFiles.map(file => `--exclude=${file}`),
    '--fileParallelism',
    `--maxWorkers=${String(workers)}`,
  ]))
} else {
  process.exitCode = serialStatus
}

function run(invocation: { command: string; args: string[] }): Promise<number> {
  return new Promise((resolveRun, reject) => {
    const child = spawn(invocation.command, invocation.args, { stdio: 'inherit' })
    child.once('error', reject)
    child.once('exit', (exitCode, signalCode) => {
      if (signalCode !== null) {
        console.error(`web snapshots terminated by ${signalCode}`)
        resolveRun(1)
        return
      }
      resolveRun(exitCode ?? 1)
    })
  })
}
