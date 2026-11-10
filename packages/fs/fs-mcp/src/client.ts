/**
 * The MCP connection to one `nc-tools-mcp` server.
 *
 * The server is the filesystem authority for this provider, so every
 * operation is one `tools/call` round trip over stdio. The client owns the
 * process, the handshake, and the JSON envelope; it does not know what any
 * tool means.
 *
 * @module @deepseek-ai/dsh-fs-mcp/client
 */

import { Client } from '@modelcontextprotocol/client'
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio'
import type { NcToolError } from './types.ts'

/** One tool call's outcome: the parsed payload, or the structured failure. */
export type NcCallOutcome =
  | { readonly ok: true; readonly value: unknown }
  | { readonly ok: false; readonly error: NcToolError }

/**
 * A live connection to an nc-tools MCP server.
 *
 * The connection is created connected: {@link NcToolsClient.connect} spawns
 * the server, performs the MCP handshake, and returns a client that is ready
 * to answer calls. {@link NcToolsClient.close} ends it and asks the transport
 * to shut the server down; neither step can observe whether the child actually
 * exited, so a close proves a request, not a reap.
 */
export class NcToolsClient {
  private constructor(
    private readonly client: Client,
    private readonly transport: StdioClientTransport,
  ) {}

  /**
   * Spawn an nc-tools server and connect to it.
   *
   * @param command - The `nc-tools-mcp` executable.
   * @param args - Arguments for the executable; the workspace root is appended.
   * @param workspaceRoot - The workspace root the server serves.
   * @returns A connected client.
   */
  static async connect(command: string, args: string[], workspaceRoot: string): Promise<NcToolsClient> {
    const transport = new StdioClientTransport({
      command,
      args: [...args, workspaceRoot],
      env: process.env as Record<string, string>,
    })
    const client = new Client(
      { name: 'dsh-fs-mcp', version: '0.0.1' },
      { capabilities: {}, versionNegotiation: { mode: 'auto' } },
    )
    await client.connect(transport)
    return new NcToolsClient(client, transport)
  }

  /**
   * Call one nc-tools tool.
   *
   * A tool failure is a VALUE here, not an exception: nc-tools reports
   * failures as a structured `{code, message, hint}` payload, and the caller
   * maps that onto the filesystem seam's error vocabulary. Only a transport
   * or protocol failure — the server died, the frame was malformed — throws.
   *
   * @param tool - The nc-tools tool name, e.g. `fs.read`.
   * @param args - The tool arguments.
   * @param timeoutMs - How long the call may take.
   * @param signal - Cancels the call.
   * @returns The tool's parsed result, or its structured failure.
   */
  async call(
    tool: string,
    args: Record<string, unknown>,
    timeoutMs: number,
    signal?: AbortSignal,
  ): Promise<NcCallOutcome> {
    const result = await this.client.callTool(
      { name: tool, arguments: args },
      { timeout: timeoutMs, ...(signal !== undefined ? { signal } : {}) },
    )
    const content = result.content as Array<{ type?: string; text?: string }>
    /* v8 ignore next 3 -- the MCP content array carries a text block for a
     * tool result, so the empty-text fallback cannot come from a server
     * that follows the spec. */
    const text = content.find(block => block.type === 'text')?.text ?? ''
    // The server sends the structured payload alongside the text block when
    // the negotiated protocol revision supports it; the text block is the
    // same JSON, so it is the fallback.
    const payload = result.structuredContent !== undefined
      ? result.structuredContent
      : parseJson(text)
    if (result.isError === true) {
      return { ok: false, error: readError(payload, text) }
    }
    return { ok: true, value: payload }
  }

  /**
   * Close the connection and reap the server process.
   *
   * @returns Nothing; the close is best-effort so a dead server does not
   *   turn a teardown into a failure.
   */
  async close(): Promise<void> {
    try {
      await this.client.close()
    } catch {
      // The server may already be gone; the transport close below still asks
      // the provider to shut the child down.
    }
    try {
      await this.transport.close()
    } catch {
      // Closing an already-closed transport is not a failure.
    }
  }
}

/**
 * Parse a text block as JSON, tolerating a non-JSON body.
 *
 * @param text - The raw text block.
 * @returns The parsed value, or the text itself.
 */
function parseJson(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return text
  }
}

/**
 * Read the structured error out of a failed call's payload.
 *
 * @param payload - The parsed result payload.
 * @param fallbackText - The text block, used when the payload is not an error.
 * @returns The structured error.
 */
function readError(payload: unknown, fallbackText: string): NcToolError {
  if (typeof payload === 'object' && payload !== null) {
    const record = payload as Record<string, unknown>
    const error = record['error']
    if (typeof error === 'object' && error !== null) {
      const detail = error as Record<string, unknown>
      const code = typeof detail['code'] === 'string' ? detail['code'] : 'ERR_INTERNAL'
      const message = typeof detail['message'] === 'string' ? detail['message'] : fallbackText
      return { code, message, ...(detail['hint'] !== undefined ? { hint: detail['hint'] } : {}) }
    }
  }
  return { code: 'ERR_INTERNAL', message: fallbackText }
}
