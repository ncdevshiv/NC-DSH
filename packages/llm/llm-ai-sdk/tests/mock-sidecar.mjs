/**
 * Scriptable mock `ai-sidecar` child for adapter tests. Speaks the sidecar
 * JSON-RPC protocol over stdio and replays the event script supplied through
 * `$MOCK_SIDECAR_SCRIPT` (a JSON array of `StreamEvent` shapes) after each
 * accepted `chat.stream`, closing with `chat/done`. The `hang` marker as the
 * sole scripted event keeps the stream open until cancelled.
 * @module mock-sidecar
 */

import { createInterface } from 'node:readline'

const script = JSON.parse(process.env.MOCK_SIDECAR_SCRIPT ?? '[]')
const failStream = process.env.MOCK_SIDECAR_FAIL === '1'
const failInitialize = process.env.MOCK_SIDECAR_FAIL_INIT === '1'

function write(frame) {
  process.stdout.write(`${JSON.stringify(frame)}\n`)
}

function respond(id, result) {
  write({ jsonrpc: '2.0', id, result })
}

function respondError(id, code, message, data) {
  write({ jsonrpc: '2.0', id, error: { code, message, data } })
}

const streams = new Map()

createInterface({ input: process.stdin }).on('line', (line) => {
  const trimmed = line.trim()
  if (trimmed.length === 0) return
  let frame
  try {
    frame = JSON.parse(trimmed)
  } catch {
    return
  }
  const { id, method, params } = frame
  if (method === undefined || id === undefined) return
  switch (method) {
    case 'initialize':
      if (failInitialize) {
        respondError(id, -32000, 'initialize refused', { kind: 'configuration', retryable: false })
      } else {
        respond(id, { protocol: 1, version: 'mock' })
      }
      break
    case 'model.discover': {
      // Mirrors the real sidecar's contract: the OpenAI-compatible dialect
      // needs an endpoint base; native dialects have SDK defaults.
      const api = params.api ?? 'openai-compatible'
      if ((params.base_url === undefined || params.base_url.length === 0)
        && api !== 'anthropic' && api !== 'google') {
        respondError(id, -32602, 'missing `base_url`', null)
        break
      }
      respond(id, {
        models: [
          {
            id: 'discovered-small',
            name: 'Discovered Small',
            context_window: 8192,
            max_output_tokens: 2048,
            capabilities: { input_modalities: ['text'] },
          },
          { id: 'discovered-large', context_window: 200000, max_output_tokens: 32768 },
        ],
      })
      break
    }
    case 'configure':
      respond(id, { ok: true, providers: Object.keys(params.providers ?? {}) })
      break
    case 'provider.list':
      respond(id, { providers: Object.keys(params.providers ?? {}) })
      break
    case 'chat.stream': {
      const streamId = params.stream_id
      respond(id, { accepted: true, stream_id: streamId })
      const emit = (event) => {
        write({ jsonrpc: '2.0', method: 'chat/event', params: { stream_id: streamId, event } })
      }
      const finish = () => {
        write({ jsonrpc: '2.0', method: 'chat/done', params: { stream_id: streamId, ok: true } })
        streams.delete(streamId)
      }
      if (script.length === 1 && script[0] === 'hang') {
        streams.set(streamId, finish)
        return
      }
      for (const event of script) emit(event)
      if (failStream) {
        write({
          jsonrpc: '2.0',
          method: 'chat/done',
          params: { stream_id: streamId, ok: false, error: { kind: 'rate_limit', message: 'slow down', retryable: true } },
        })
      } else {
        finish()
      }
      break
    }
    case 'stream.cancel': {
      const finish = streams.get(params.stream_id)
      if (finish !== undefined) {
        streams.delete(params.stream_id)
        // Cancelled streams close silently; the host is no longer listening.
      }
      respond(id, { cancelled: finish !== undefined })
      break
    }
    default:
      respondError(id, -32601, `unknown method \`${method}\``, null)
  }
})
