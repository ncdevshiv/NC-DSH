/**
 * Translation between harness messages/chunks and the sidecar's
 * `ai_types::Message` / `StreamEvent` JSON. Pure functions: request assembly
 * reads already-resolved attachment bytes; the event assembler is a state
 * machine over one stream's notifications.
 * @module @deepseek-ai/dsh-llm-ai-sdk/translate
 */

import { CallId } from '@deepseek-ai/dsh-llm'
import type {
  ContentBlock,
  FinishReason,
  GenerateOptions,
  Message,
  StreamChunk,
  TextBlock,
  ToolCallBlock,
} from '@deepseek-ai/dsh-llm'
import type { SidecarStreamEvent } from './types.ts'

/** One wire content part of an `ai_types::Message`. */
type WirePart =
  | { type: 'text'; text: string }
  | { type: 'image'; image: { source: 'base64'; media_type: string; data: string } }
  | { type: 'tool_call'; call: { id: string; name: string; arguments: string } }
  | { type: 'tool_result'; result: { id: string; name: string; output: string; is_error: boolean } }

/** One `ai_types::Message` on the sidecar request payload. */
export interface WireMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  parts: WirePart[]
}

/** Resolved image bytes for one image block, keyed by attachment id. */
export type ResolvedImages = ReadonlyMap<string, { mediaType: string; base64: string }>

/** The `chat.*` request body sent to the sidecar. */
export interface WireChatRequest {
  messages: WireMessage[]
  tools?: { name: string; description: string; input_schema: unknown }[]
  temperature?: number
  max_tokens?: number
  stop?: string[]
  reasoning_effort?: 'low' | 'medium' | 'high' | 'max'
  provider_options?: Record<string, unknown>
}

/**
 * Flatten one harness message's blocks into wire parts.
 *
 * Text, reasoning (as the assistant's own prior text), and tool-call blocks
 * translate directly; tool-result messages become a single `tool`-role message
 * carrying their flattened result content. Image blocks require pre-resolved
 * bytes in `images`; unresolved ones fail loud rather than silently dropping
 * user content.
 */
function partsOf(message: Message, images: ResolvedImages): { role: WireMessage['role']; parts: WirePart[] } {
  const parts: WirePart[] = []
  const pushBlocks = (blocks: readonly ContentBlock[]): void => {
    for (const block of blocks) {
      switch (block.type) {
        case 'text': {
          if (block.text.length > 0) parts.push({ type: 'text', text: block.text })
          break
        }
        case 'reasoning': {
          if (block.text.length > 0) parts.push({ type: 'text', text: block.text })
          break
        }
        case 'image': {
          const resolved = images.get(block.attachment.attachmentId)
          if (resolved === undefined) {
            throw new Error(
              `llm-ai-sdk: image attachment "${block.attachment.attachmentId}" was not pre-resolved for the request`,
            )
          }
          parts.push({
            type: 'image',
            image: { source: 'base64', media_type: resolved.mediaType, data: resolved.base64 },
          })
          break
        }
        case 'tool-call': {
          parts.push({ type: 'tool_call', call: { id: block.id, name: block.name, arguments: block.arguments } })
          break
        }
        case 'tool-result': {
          // The sidecar's ToolResult carries plain-text output; nested blocks
          // flatten to their text so nothing model-visible is dropped.
          const output = (block.content as readonly ContentBlock[]).map((b: ContentBlock) => flattenText([b])).join('\n')
          parts.push({
            type: 'tool_result',
            result: { id: block.toolCallId, name: '', output, is_error: block.isError === true },
          })
          break
        }
      }
    }
  }

  pushBlocks(message.content)
  // A tool-result message keeps its user role only when it carries content
  // beyond the result itself; a bare result travels as the wire `tool` role.
  const onlyResult = message.source.kind === 'tool'
    && message.content.length === 1
    && message.content[0]?.type === 'tool-result'
  return { role: onlyResult ? 'tool' : message.role, parts }
}

/** Concatenated text of one content list (non-text blocks contribute nothing). */
function flattenText(blocks: readonly ContentBlock[]): string {
  return blocks.filter((block): block is TextBlock => block.type === 'text').map((block: TextBlock) => block.text).join('')
}

/**
 * Assemble the sidecar chat request for one prepared call.
 * @param options - the fully-assembled harness request.
 * @param images - attachment bytes resolved by the adapter before assembly.
 * @returns the wire body mirroring `ai_core::ChatRequest`.
 */
export function toChatRequest(options: GenerateOptions, images: ResolvedImages): WireChatRequest {
  const messages: WireMessage[] = []
  if (options.system !== undefined && options.system.length > 0) {
    messages.push({ role: 'system', parts: [{ type: 'text', text: options.system }] })
  }
  for (const message of options.messages) {
    const { role, parts } = partsOf(message, images)
    if (parts.length > 0) messages.push({ role, parts })
  }
  if (messages.length === 0) {
    throw new Error('llm-ai-sdk: request must contain at least one non-empty message')
  }
  const effort = reasoningEffortOf(options)
  return {
    messages,
    ...(options.tools === undefined || options.tools.length === 0 ? {} : {
      tools: options.tools.map(tool => ({
        name: tool.name,
        description: tool.description,
        input_schema: tool.parameters,
      })),
    }),
    ...(options.temperature === undefined ? {} : { temperature: options.temperature }),
    ...(options.maxTokens === undefined ? {} : { max_tokens: options.maxTokens }),
    ...(options.stop === undefined || options.stop.length === 0 ? {} : { stop: [...options.stop] }),
    ...(effort === undefined ? {} : { reasoning_effort: effort }),
    ...(options.providerOptions === undefined ? {} : { provider_options: options.providerOptions }),
  }
}

/** Harness effort vocabulary projected onto the sidecar's four levels; `off` keeps the provider default. */
function reasoningEffortOf(options: GenerateOptions): 'low' | 'medium' | 'high' | 'max' | undefined {
  switch (options.reasoningEffort) {
    case 'low': return 'low'
    case 'medium': return 'medium'
    case 'high': return 'high'
    case 'max': return 'max'
    default: return undefined
  }
}

/**
 * Map the provider's finish-reason spelling onto the harness vocabulary.
 * Unrecognized reasons fall through to `stop` (merge-extensible union).
 * @param reason - the sidecar's terminal `finish_reason`, when it sent one.
 * @returns the harness `FinishReason` for the stream's terminal chunk.
 */
export function finishReasonOf(reason: string | null | undefined): FinishReason {
  switch (reason) {
    case 'tool_calls':
    case 'tool_use':
      return { kind: 'tool-calls' }
    case 'length':
    case 'max_tokens':
      return { kind: 'max-tokens' }
    default:
      return { kind: 'stop' }
  }
}

interface OpenProseBlock {
  index: number
  kind: 'text' | 'reasoning'
  text: string
}

interface OpenToolBlock {
  index: number
  id: string
  name: string
  arguments: string
}

/**
 * Assemble harness chunks from one stream's sidecar events. Block starts and
 * ends are derived here: the sidecar reports flat events, while the adapter
 * protocol requires correlated `block-start`/`block-end` frames around every
 * assembled block.
 */
export class StreamAssembler {
  private nextIndex = 0
  private prose: OpenProseBlock | undefined
  private tool: OpenToolBlock | undefined
  private usage: Extract<SidecarStreamEvent, { type: 'usage_update' }>['usage'] | undefined

  /**
   * Fold one sidecar event into zero or more chunks.
   * @param event - the serde-tagged event from `chat/event`.
   * @returns the chunks to yield, in order.
   */
  accept(event: SidecarStreamEvent): StreamChunk[] {
    // The literal objects below already conform to {@link StreamChunk};
    // TS widens `name` to `string | undefined` so cast the array back.
    switch (event.type) {
      case 'text_delta':
        return this.proseDelta('text', event.delta)
      case 'reasoning_delta':
        return this.proseDelta('reasoning', event.delta)
      case 'tool_call_started': {
        const closed = this.closeProse()
        this.tool = { index: this.nextIndex++, id: event.id, name: event.name, arguments: '' }
        return [
          ...closed,
          { type: 'block-start', index: this.tool.index, blockType: 'tool-call' },
          { type: 'tool-call-delta', id: CallId(event.id), name: event.name, argumentsDelta: '' },
        ] as StreamChunk[] as unknown as StreamChunk[]
      }
      case 'tool_call_delta': {
        if (this.tool !== undefined && this.tool.id === event.id) {
          this.tool.arguments += event.arguments_delta
        }
        return [{
          type: 'tool-call-delta',
          id: CallId(event.id),
          argumentsDelta: event.arguments_delta,
        } as StreamChunk]
      }
      case 'tool_call_completed': {
        const block: ToolCallBlock = {
          type: 'tool-call',
          id: CallId(event.call.id),
          name: event.call.name,
          arguments: event.call.arguments,
        }
        if (this.tool !== undefined && this.tool.id === event.call.id) {
          const index = this.tool.index
          this.tool = undefined
          return [{ type: 'block-end', index, block }]
        }
        // A completion without a matching start still closes authoritatively;
        // the core assembler accepts a block-end that opens its own block.
        return [{ type: 'block-end', index: this.nextIndex++, block }]
      }
      case 'usage_update':
        // Deferred to stream end so usage never lands inside an open block:
        // providers attach it before or after their terminal content event,
        // and both orderings must assemble identically.
        this.usage = event.usage
        return []
      case 'error': {
        // A mid-stream recoverable error carries no failure facts; surface it
        // as reasoning text so the note stays visible without faking a finish.
        const opened = this.prose?.kind === 'reasoning'
        const closed = opened ? [] : this.closeProse()
        if (!opened) {
          this.prose = { index: this.nextIndex++, kind: 'reasoning', text: '' }
        }
        const prose = this.prose
        if (prose === undefined) {
          // The branch above just assigned it; unreachable in practice.
          return closed
        }
        prose.text += event.message
        return [
          ...closed,
          ...(opened ? [] : [{ type: 'block-start', index: prose.index, blockType: 'reasoning' } as StreamChunk]),
          { type: 'reasoning-delta', index: prose.index, text: event.message },
        ]
      }
      case 'completed':
        return [...this.closeProse(), ...this.flushUsage()]
    }
  }

  /** Close any open block, report deferred usage, and close before termination.
   * @returns the closing `block-end`/`usage` chunks, in emission order.
   */
  close(): StreamChunk[] {
    return [...this.closeProse(), ...this.flushUsage(), ...this.closeTool()]
  }

  private flushUsage(): StreamChunk[] {
    const usage = this.usage
    if (usage === undefined) return []
    this.usage = undefined
    const cached = usage.cached_input_tokens ?? undefined
    return [{
      type: 'usage',
      usage: {
        // Counts are disjoint per the seam contract: the sidecar's
        // input_tokens folds cache hits in, so they are subtracted out.
        inputTokens: Math.max(0, usage.input_tokens - (cached ?? 0)),
        outputTokens: usage.output_tokens,
        ...(cached === undefined ? {} : { cacheReadTokens: cached }),
        ...(usage.reasoning_tokens == null ? {} : { reasoningTokens: usage.reasoning_tokens }),
      },
    }]
  }

  /** An unterminated tool call still owes the harness its assembled block. */
  private closeTool(): StreamChunk[] {
    const tool = this.tool
    if (tool === undefined) return []
    this.tool = undefined
    const block: ToolCallBlock = {
      type: 'tool-call',
      id: CallId(tool.id),
      name: tool.name,
      arguments: tool.arguments,
    }
    return [{ type: 'block-end', index: tool.index, block }]
  }

  private proseDelta(kind: 'text' | 'reasoning', delta: string): StreamChunk[] {
    if (this.prose !== undefined && this.prose.kind === kind) {
      this.prose.text += delta
      return [{ type: kind === 'text' ? 'text-delta' : 'reasoning-delta', index: this.prose.index, text: delta }]
    }
    const closed = this.closeProse()
    this.prose = { index: this.nextIndex++, kind, text: delta }
    return [
      ...closed,
      { type: 'block-start', index: this.prose.index, blockType: kind },
      { type: kind === 'text' ? 'text-delta' : 'reasoning-delta', index: this.prose.index, text: delta },
    ]
  }

  private closeProse(): StreamChunk[] {
    const block = this.prose
    if (block === undefined) return []
    this.prose = undefined
    const emitted: ContentBlock = block.kind === 'text'
      ? { type: 'text', text: block.text }
      : { type: 'reasoning', text: block.text }
    return [{ type: 'block-end', index: block.index, block: emitted }]
  }
}
