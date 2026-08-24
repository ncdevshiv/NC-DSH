/**
 * `MoliFetchProvider`: a `WebFetchProvider` that retrieves one URL through the
 * local [moli](https://github.com/lexmount/moli) headless-browser CLI
 * (`fetch --dump markdown`), so JavaScript-rendered pages return rendered
 * markdown instead of the empty shells a plain HTTP GET sees. Availability is
 * a memoized local `--version` probe; retrieval runs moli as a shell-free
 * subprocess with abort propagation.
 * @module @deepseek-ai/dsh-web-fetch-moli/provider
 */

import { spawnSync } from 'node:child_process'
import { WebError } from '@deepseek-ai/dsh-web'
import type {
  WebFetchProvider,
  WebFetchRequest,
  WebFetchResult,
} from '@deepseek-ai/dsh-web'
import type { NativeCommandRunner } from '@deepseek-ai/dsh-native-command'
import { runNativeCommand } from '@deepseek-ai/dsh-native-command'
import type { MoliBinaryProbe, MoliBinaryProber, MoliFetchProviderOptions } from './types.ts'
import { validateFetchUrl } from './policy.ts'

/** Stable id this provider registers under. */
export const MOLI_FETCH_PROVIDER_ID = 'moli'

/** Longest stderr tail quoted in a failure message. */
const MAX_STDERR_TAIL_CHARS = 500

/**
 * Validate a request URL against the same transport hygiene the local HTTP(S)
 * provider enforces before any subprocess starts. Reimplemented locally rather
 * than imported across package `src` boundaries.
 *
 * @param input - the raw URL string from the fetch request.
 * @returns the parsed `URL`.
 * @throws WebError `WEB_INVALID_URL` for malformed, non-http(s), or over-long URLs.
 * @throws WebError `WEB_BLOCKED_URL` when credentials are embedded in the URL.
 */
export { validateFetchUrl }

/**
 * The default prober: one synchronous `spawnSync([binary, '--version'])`.
 *
 * @param binaryPath - the moli executable to probe.
 * @param timeoutMs - kill budget for the probe.
 * @returns the probe result with exit status and optional spawn error.
 */
export const defaultMoliProber: MoliBinaryProber = (binaryPath, timeoutMs): MoliBinaryProbe => {
  const result = spawnSync(binaryPath, ['--version'], { timeout: timeoutMs, windowsHide: true })
  return { status: result.status, error: result.error ?? null }
}

/**
 * The moli-backed fetch provider. One provider instance may serve many fetches;
 * each fetch spawns an isolated `moli fetch` process. Markdown rides the seam's
 * `kind: 'text'` body — markdown IS text, and `dsh-tool-web` passes text through
 * untouched — until the closed `WebFetchBody` union gains a markdown arm.
 */
export class MoliFetchProvider implements WebFetchProvider {
  readonly id = MOLI_FETCH_PROVIDER_ID

  private readonly runner: NativeCommandRunner
  private readonly prober: MoliBinaryProber
  private availability: boolean | undefined

  constructor(private readonly options: MoliFetchProviderOptions) {
    this.runner = options.runner ?? runNativeCommand
    this.prober = options.prober ?? defaultMoliProber
  }

  /**
   * Cheap local usability check: the configured binary exists and runs. The
   * probe runs at most once per provider instance and blocks up to the
   * configured probe budget on that first call.
   * @returns true when `moli --version` exited successfully.
   */
  available(): boolean {
    if (this.availability === undefined) {
      const probe = this.prober(this.options.binaryPath, this.options.probeTimeoutMs)
      this.availability = probe.status === 0 && probe.error === null
    }
    return this.availability
  }

  /**
   * Retrieve one URL by rendering it in moli and returning the dumped markdown.
   * A genuinely failed navigation surfaces as an error instead of a status code:
   * dump mode does not expose the HTTP status, so every success reports `200`
   * (documented approximation).
   *
   * @param request - the URL to retrieve.
   * @param signal - caller cancellation; composed with the configured backstop budget.
   * @returns the fetched markdown capped to `maxBodyChars`.
   */
  async fetch(request: WebFetchRequest, signal?: AbortSignal): Promise<WebFetchResult> {
    validateFetchUrl(request.url, this.options.maxUrlLength)
    // A caller that already aborted must not launch a doomed subprocess; the
    // composed backstop below would kill it, but only after a spawn.
    if (signal?.aborted) {
      throw new WebError('moli fetch aborted', 'WEB_ABORTED')
    }
    const backstop = AbortSignal.timeout(this.options.timeoutMs)
    const merged = signal !== undefined ? AbortSignal.any([signal, backstop]) : backstop
    let stdout: string
    try {
      const captured = await this.runner(
        this.options.binaryPath,
        ['fetch', '--dump', 'markdown', '--wait-until', 'done', request.url],
        merged,
      )
      stdout = captured.stdout
    } catch (error: unknown) {
      throw this.classifyFailure(error, signal, backstop)
    }
    if (stdout.length === 0) {
      throw new WebError('moli produced no content', 'WEB_PROVIDER_ERROR')
    }
    const truncated = stdout.length > this.options.maxBodyChars
    const content = truncated ? stdout.slice(0, this.options.maxBodyChars) : stdout
    return {
      url: request.url,
      statusCode: 200,
      body: { kind: 'text', content },
      truncated,
    }
  }

  /**
  * Map one runner failure to its seam error. The signals decide first —
  * cancellation stays cancellation (`WEB_ABORTED`) and an exhausted backstop
  * becomes `WEB_FETCH_TIMEOUT` — because a runner may reject with any shape
  * when its process is killed; only abort-shaped errors neither signal
  * explains are classified from the thrown value. Then a missing binary names
  * the fix, and everything else quotes the stderr tail.
  *
  * @param error - the thrown runner failure.
  * @param callerSignal - the caller's own signal, checked first.
  * @param backstop - the timeout signal composed into the run.
  * @returns the classified {@link WebError}.
  */
  private classifyFailure(error: unknown, callerSignal: AbortSignal | undefined, backstop: AbortSignal): WebError {
    if (callerSignal?.aborted) {
      return new WebError('moli fetch aborted', 'WEB_ABORTED', { cause: error })
    }
    if (backstop.aborted) {
      return new WebError(`moli fetch timed out after ${this.options.timeoutMs}ms`, 'WEB_FETCH_TIMEOUT', { cause: error })
    }
    if (isAbortError(error)) {
      return new WebError('moli fetch aborted', 'WEB_ABORTED', { cause: error })
    }
    if (isMissingBinary(error)) {
      return new WebError(
        `the moli binary was not found at "${this.options.binaryPath}" — install moli or point binaryPath/$MOLI_BINARY at it`,
        'WEB_PROVIDER_ERROR',
        { cause: error },
      )
    }
    const stderr = tail((error as { stderr?: string }).stderr, MAX_STDERR_TAIL_CHARS)
    const detail = stderr.length > 0 ? stderr : String(error)
    return new WebError(`moli fetch failed: ${detail}`, 'WEB_PROVIDER_ERROR', { cause: error })
  }
}

/** True for a fetch/`AbortSignal` abort raised as a DOMException. */
function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === 'AbortError'
}

/** True when the runner could not launch the binary at all (`ENOENT`). */
function isMissingBinary(error: unknown): boolean {
  return (error as { code?: unknown }).code === 'ENOENT'
}

/** Keep only the last `maxChars` characters of a possibly-empty string. */
function tail(value: string | undefined, maxChars: number): string {
  const text = value ?? ''
  return text.length > maxChars ? text.slice(-maxChars) : text
}
