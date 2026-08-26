/**
 * In-process fake GitHub Releases server for the sidecar-update suites. It
 * serves one latest-release document whose asset URLs point back at itself,
 * plus the asset bytes and the SHA256SUMS manifest, so the real network layer
 * (global fetch) is exercised against a local socket.
 */

import { createHash } from 'node:crypto'
import { createServer, type Server } from 'node:http'

/** The release-asset filename this host would select. */
export function hostAssetName(prefix = 'ai-sidecar'): string {
  const suffix = process.platform === 'win32' ? '.exe' : ''
  return `${prefix}-${process.platform}-${process.arch}${suffix}`
}

/** Options shaping the served release; every field has a usable default. */
export interface FakeGithubOptions {
  /** Release tag served as `tag_name`. */
  tag?: string
  /** HTTP status answered for the release lookup (e.g. 404). */
  releaseStatus?: number
  /** Raw body overriding the generated JSON document. */
  releaseBody?: string
  /** Extra malformed rows injected into the assets array. */
  extraAssetRows?: unknown[]
  /** Bytes served for the binary asset. */
  assetBytes?: Uint8Array
  /** Content served for SHA256SUMS; defaults to a correct manifest. */
  sumsText?: string
  /** Omit the SHA256SUMS asset from the release document. */
  omitSums?: boolean
  /** Omit the binary asset from the release document (only SHA256SUMS remains). */
  omitBinary?: boolean
  /** Serve only tag_name and assets: no name, published_at, or html_url. */
  minimalRelease?: boolean
  /** HTTP status answered for asset downloads. */
  downloadStatus?: number
}

/** One running fake server. */
export interface FakeGithub {
  /** Base URL answering release requests (`http://127.0.0.1:<port>`). */
  readonly url: string
  /** Every request path seen so far, in order. */
  readonly requests: readonly string[]
  /** Stop listening. */
  close(): Promise<void>
}

/**
 * Start one fake GitHub server.
 * @param options - release shape overrides.
 * @returns the running server handle.
 */
export async function startFakeGithub(options: FakeGithubOptions = {}): Promise<FakeGithub> {
  const tag = options.tag ?? 'v1.2.0'
  const assetName = hostAssetName()
  const bytes = options.assetBytes ?? new Uint8Array([0x4d, 0x5a, 1, 2, 3, 4, 5])
  const digest = createHash('sha256').update(bytes).digest('hex')
  const sumsText = options.sumsText ?? `${digest}  ${assetName}\n`
  const requests: string[] = []
  const server: Server = createServer((request, response) => {
    const path = request.url ?? '/'
    requests.push(path)
    if (path === '/repos/owner/repo/releases/latest') {
      if (options.releaseBody !== undefined) {
        response.writeHead(options.releaseStatus ?? 200, { 'content-type': 'application/json' })
        response.end(options.releaseBody)
        return
      }
      if (options.releaseStatus !== undefined) {
        response.writeHead(options.releaseStatus)
        response.end('{}')
        return
      }
      const assets: unknown[] = [
        ...(options.extraAssetRows ?? []),
      ]
      if (!options.omitBinary) {
        assets.push({ name: assetName, browser_download_url: `${selfUrl(request)}/assets/${assetName}` })
      }
      if (!options.omitSums) {
        assets.push({ name: 'SHA256SUMS', browser_download_url: `${selfUrl(request)}/assets/SHA256SUMS` })
      }
      response.writeHead(200, { 'content-type': 'application/json' })
      if (options.minimalRelease === true) {
        response.end(JSON.stringify({ tag_name: tag, assets }))
        return
      }
      response.end(JSON.stringify({
        tag_name: tag,
        name: `Release ${tag}`,
        html_url: `https://example.invalid/releases/${tag}`,
        published_at: '2026-08-01T00:00:00Z',
        assets,
      }))
      return
    }
    if (path === `/assets/${assetName}`) {
      if (options.downloadStatus !== undefined) {
        response.writeHead(options.downloadStatus)
        response.end('nope')
        return
      }
      response.writeHead(200)
      response.end(Buffer.from(bytes))
      return
    }
    if (path === '/assets/SHA256SUMS') {
      response.writeHead(200, { 'content-type': 'text/plain' })
      response.end(sumsText)
      return
    }
    response.writeHead(404)
    response.end('not found')
  })
  await new Promise<void>((resolveListen) => {
    server.listen(0, '127.0.0.1', () => resolveListen())
  })
  const address = server.address()
  if (address === null || typeof address === 'string') throw new Error('fake github failed to bind')
  return {
    url: `http://127.0.0.1:${String(address.port)}`,
    requests,
    close: () => new Promise<void>((resolveClose, rejectClose) => {
      server.close(error => error === undefined ? resolveClose() : rejectClose(error))
    }),
  }
}

/** Rebuild the absolute self URL for asset links from the incoming request. */
function selfUrl(request: import('node:http').IncomingMessage): string {
  const address = request.socket.localAddress ?? '127.0.0.1'
  const port = request.socket.localPort ?? 80
  const host = address.includes(':') ? `[${address}]` : address
  return `http://${host}:${String(port)}`
}
