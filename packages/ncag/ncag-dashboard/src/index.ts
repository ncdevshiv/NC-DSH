/**
 * The NC-AG control plane: an HTTP API over the orchestrator, a server-sent
 * event stream of platform activity, and the operational dashboard the API
 * serves.
 *
 * Every route performs the operation it advertises: the dashboard renders only
 * data the API returned, and every control calls the same orchestrator method
 * the command line calls.
 * @module @deepseek-ai/dsh-ncag-dashboard
 */

import { createServer } from 'node:http'
import type { IncomingMessage, Server, ServerResponse } from 'node:http'
import { DASHBOARD_HTML } from './ui.ts'
import { handleRoute } from './routes.ts'
import type { RouteContext } from './routes.ts'
import type { NcagOrchestrator } from '@deepseek-ai/dsh-ncag-orchestrator'

/** Dashboard configuration. */
export interface DashboardConfig {
  /** Port to listen on; 0 selects a free port. */
  readonly port: number
  /** Interface to bind; loopback by default. */
  readonly host: string
  /** Bearer token required on every API call; absent disables authentication for loopback-only use. */
  readonly token: string | null
}

/** A running dashboard. */
export interface Dashboard {
  /** The URL the dashboard is served on. */
  readonly url: string
  /** Stop the server and close every client stream. */
  stop(): Promise<void>
}

/** Parse JSON from a request body with a bounded size. */
function readJson(request: IncomingMessage): Promise<unknown> {
  return new Promise((resolve, reject) => {
    let size = 0
    const chunks: Buffer[] = []
    request.on('data', (chunk: Buffer) => {
      size += chunk.length
      if (size > 1_000_000) {
        reject(new Error('request body exceeds 1 MB'))
        request.destroy()
        return
      }
      chunks.push(chunk)
    })
    request.on('end', () => {
      if (chunks.length === 0) {
        resolve({})
        return
      }
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString('utf8')) as unknown)
      } catch (error: unknown) {
        reject(new Error(`invalid JSON body: ${(error as Error).message}`))
      }
    })
    request.on('error', reject)
  })
}

/**
 * Start the dashboard over one orchestrator.
 * @param orchestrator - the platform the dashboard observes and controls.
 * @param config - bind address, port and API token.
 * @returns the running dashboard.
 */
export async function startDashboard(orchestrator: NcagOrchestrator, config: DashboardConfig): Promise<Dashboard> {
  const streams = new Set<ServerResponse>()
  const unsubscribe = orchestrator.store.subscribe((event) => {
    const payload = `data: ${JSON.stringify(event)}\n\n`
    for (const stream of streams) stream.write(payload)
  })
  const context: RouteContext = {
    orchestrator,
    token: config.token,
    readJson,
    subscribe: (response) => {
      streams.add(response)
      return () => {
        streams.delete(response)
      }
    },
  }
  const server: Server = createServer((request, response) => {
    void (async () => {
      try {
        await handleRoute(context, request, response, DASHBOARD_HTML)
      } catch (error: unknown) {
        /* v8 ignore start -- route handlers raise Error and write no headers before failing; the alternative branches are defensive. */
        const message = error instanceof Error ? error.message : String(error)
        if (!response.headersSent) {
          response.writeHead(500, { 'content-type': 'application/json' })
        }
        response.end(JSON.stringify({ error: message }))
        /* v8 ignore stop */
      }
    })()
  })
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject)
    server.listen(config.port, config.host, () => {
      server.off('error', reject)
      resolve()
    })
  })
  const address = server.address()
  /* v8 ignore next -- a listening server always reports an AddressInfo object. */
  const port = typeof address === 'object' && address !== null ? address.port : config.port
  const host = config.host === '0.0.0.0' || config.host === '::' ? '127.0.0.1' : config.host
  return {
    url: `http://${host}:${port}/`,
    stop: async () => {
      unsubscribe()
      for (const stream of streams) stream.end()
      streams.clear()
      await new Promise<void>((resolve) => { server.close(() =>{  resolve() }) })
    },
  }
}

export { DASHBOARD_HTML } from './ui.ts'
export * from './platform.ts'
