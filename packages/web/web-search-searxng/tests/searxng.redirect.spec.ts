/**
 * Real HTTP coverage proves whether native `fetch` contacts a cross-origin `Location`; mocked
 * request-init assertions alone cannot observe that boundary. The SearXNG request carries the
 * optional basic-auth pair, so this suite also proves the credential is not forwarded.
 */

import { afterAll, beforeAll, describe, expect, it } from 'vitest'
import { createServer, type IncomingMessage, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { SearxngSearchProvider } from '@deepseek-ai/dsh-web-search-searxng'

const TEST_USERNAME = 'redirect-user'
const TEST_PASSWORD = 'redirect-password'
const TEST_QUERY = 'private redirect query'
const targetRequests: ReceivedRequest[] = []

interface ReceivedRequest {
  readonly headers: IncomingMessage['headers']
}

let redirectOrigin: string
let targetOrigin: string

const targetServer = createServer((request, response) => {
  void captureRequest(request).then((received) => {
    targetRequests.push(received)
    response.writeHead(204).end()
  }, (error: unknown) => response.destroy(asError(error)))
})

const redirectServer = createServer((request, response) => {
  request.resume()
  const status = Number(new URL(request.url ?? '/', 'http://fixture.test').pathname.split('/')[1])
  response.writeHead(status, { location: `${targetOrigin}/collect` }).end()
})

beforeAll(async () => {
  targetOrigin = await listen(targetServer)
  redirectOrigin = await listen(redirectServer)
})

afterAll(async () => {
  await Promise.all([close(redirectServer), close(targetServer)])
})

describe('SearxngSearchProvider redirect policy', () => {
  it.each([301, 302, 303, 307, 308])('rejects HTTP %i before contacting Location', async (status) => {
    targetRequests.length = 0
    const provider = new SearxngSearchProvider({
      baseURL: `${redirectOrigin}/${status}`,
      username: TEST_USERNAME,
      password: TEST_PASSWORD,
    })

    await expect(provider.search({ query: TEST_QUERY }))
      .rejects.toMatchObject({ code: 'WEB_PROVIDER_ERROR' })
    expect(targetRequests).toHaveLength(0)
  })

  it('shows default 307 following forwards request data the provider refuses to forward', async () => {
    // Native fetch drops `authorization` itself when a cross-origin redirect
    // follows; a custom header proves the fixture forwards everything else.
    targetRequests.length = 0
    await fetch(`${redirectOrigin}/307/search?q=${encodeURIComponent(TEST_QUERY)}&format=json`, {
      headers: { 'x-redirect-proof': 'forwarded-by-default' },
    })

    expect(targetRequests).toHaveLength(1)
    expect(targetRequests[0]?.headers['x-redirect-proof']).toBe('forwarded-by-default')
  })
})

/** Read a complete request received by the redirect target. */
function captureRequest(request: IncomingMessage): Promise<ReceivedRequest> {
  return new Promise((resolve, reject) => {
    request.resume()
    request.once('error', reject)
    request.once('end', () => { resolve({ headers: request.headers }) })
  })
}

/** Listen on an ephemeral loopback port and return the server origin. */
async function listen(server: Server): Promise<string> {
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject)
    server.listen(0, '127.0.0.1', resolve)
  })
  const address = server.address() as AddressInfo
  return `http://127.0.0.1:${address.port}`
}

/** Close a listening fixture server after every request has settled. */
async function close(server: Server): Promise<void> {
  if (!server.listening) return
  await new Promise<void>((resolve, reject) => server.close((error) => {
    if (error === undefined) resolve()
    else reject(error)
  }))
}

/** Normalize an unknown fixture failure for `ServerResponse.destroy`. */
function asError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error))
}
