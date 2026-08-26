/**
 * Default-browser handoff for `dsh web`.
 *
 * Two runtimes, one contract: hand the URL to the operating system's default
 * handler without leaking harness credentials into any spawned process.
 * Under Bun, `Bun.open` performs the launch in-process (no child process at
 * all). Under Node, a per-platform opener binary is spawned directly with
 * {@link scrubbedParentEnv}; its stderr first line becomes the manual-URL
 * diagnostic reason. The npm `open` package this module replaces paid a
 * ~500 ms PowerShell round trip per Windows launch; both paths here stay
 * under ~50 ms.
 * @module
 */

import { spawn, type ChildProcess, type SpawnOptions } from 'node:child_process'
import { scrubbedParentEnv } from '@deepseek-ai/dsh-subprocess'

/** Minimal structural view of the Bun global; `open` is only probed, never assumed. */
interface BunGlobal {
  open?: (target: string) => Promise<unknown>
}

/** True when this runtime provides the built-in `Bun.open`.
 * @returns whether the Bun global exposes a callable `open`.
 */
export function probeBunOpen(): boolean {
  const bun = (globalThis as { Bun?: BunGlobal }).Bun
  return typeof bun?.open === 'function'
}

/** Command plan for handing one URL to the platform's default handler. */
export interface OpenerCommand {
  command: string
  args: readonly string[]
  options: Pick<SpawnOptions, 'windowsHide'>
}

/**
 * Build the per-platform opener argv for one URL.
 *
 * Windows delegates to `cmd /c start`; the empty string is the window-title
 * placeholder `start` requires so a quoted target is never parsed as the
 * title. macOS uses `/usr/bin/open`; Linux and FreeBSD use `xdg-open`, which
 * fails loudly when no freedesktop opener is installed.
 * @param platform - Node platform identifier (`process.platform` in production).
 * @param url - the absolute `http(s)` URL of the ready Web surface.
 * @returns the command, argument vector, and platform-specific spawn options.
 * @throws when the platform has no supported opener.
 */
export function buildPlatformOpenerCommand(platform: NodeJS.Platform, url: string): OpenerCommand {
  switch (platform) {
    case 'win32':
      return { command: 'cmd', args: ['/c', 'start', '', url], options: { windowsHide: true } }
    case 'darwin':
      return { command: '/usr/bin/open', args: [url], options: {} }
    case 'linux':
    case 'freebsd':
      return { command: 'xdg-open', args: [url], options: {} }
    default:
      throw new Error(`no default-browser opener for platform ${platform}`)
  }
}

/**
 * Hand one URL to the platform opener and wait until the handoff either
 * completed or failed observably. The opener inherits only the scrubbed
 * parent environment: credential-shaped names and every `DSH_*` name are
 * absent from the spawned process.
 * @param url - the absolute URL to open.
 * @param platform - platform selector; defaults to the running platform.
 * @param spawnFn - spawn seam for tests; defaults to `node:child_process.spawn`.
 * @returns resolves once the opener exited zero (the handoff completed).
 * @throws the opener's first stderr line (Error-prefix stripped), or
 * `browser launcher exited with code N` when it failed silently, or the
 * spawn error itself.
 */
export function openWithChildProcess(
  url: string,
  platform: NodeJS.Platform = process.platform,
  spawnFn: typeof spawn = spawn,
): Promise<void> {
  let plan: OpenerCommand
  try {
    plan = buildPlatformOpenerCommand(platform, url)
  } catch (error) {
    return Promise.reject(error instanceof Error ? error : new Error(String(error)))
  }

  return new Promise<void>((resolve, reject) => {
    const launcher: ChildProcess = spawnFn(plan.command, plan.args, {
      env: scrubbedParentEnv(),
      stdio: ['ignore', 'ignore', 'pipe'],
      ...plan.options,
    })
    let launcherStderr = ''
    launcher.stderr?.setEncoding('utf8')
    launcher.stderr?.on('data', (chunk: string) => {
      launcherStderr += chunk
    })
    function onError(error: Error): void {
      launcher.off('exit', onExit)
      reject(error)
    }
    // Settle on `exit`, not `close`: the Windows `start` route can leave the
    // launcher's stdio pipes in the hands of the process it dispatched (a
    // console-hosted target inherits them), so pipe drain (`close`) has no
    // bound while process lifetime (`exit`) does. Stderr collected up to exit
    // is still the failure reason.
    function onExit(code: number | null): void {
      launcher.off('error', onError)
      launcher.stderr?.removeAllListeners()
      if (code !== 0) {
        const firstLine = launcherStderr.trim().split(/\r?\n/u)[0]
        const reason = firstLine === undefined || firstLine === ''
          ? `browser launcher exited with code ${String(code)}`
          : firstLine.replace(/^(?:[A-Za-z]*Error):\s*/u, '')
        reject(new Error(reason))
        return
      }
      // Opener chatter (e.g. xdg-open warnings) still reaches the operator.
      if (launcherStderr !== '') process.stderr.write(launcherStderr)
      resolve()
    }
    launcher.once('error', onError)
    launcher.once('exit', onExit)
  })
}

/**
 * Hand one URL to the operating system's default browser.
 *
 * Prefers the runtime's built-in `Bun.open` (in-process, detached, stdio-free);
 * falls back to {@link openWithChildProcess} on Node runtimes.
 * @param url - the absolute URL of the ready Web surface.
 * @throws the same failures as the selected backend; callers turn them into
 * the manual-URL warning.
 */
export async function openDefaultBrowser(url: string): Promise<void> {
  const bun = (globalThis as { Bun?: BunGlobal }).Bun
  if (typeof bun?.open === 'function') {
    await bun.open(url)
    return
  }
  await openWithChildProcess(url)
}
