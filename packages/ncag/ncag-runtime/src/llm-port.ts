/**
 * Production model port: adapts the harness LLM service to the runtime's
 * provider-neutral request and response, so an NC-AG agent streams through the
 * same registered provider routes the rest of the harness uses.
 * @module @deepseek-ai/dsh-ncag-runtime/llm-port
 */

import { createAssistantMessage, createToolResultMessage, createUserMessage } from '@deepseek-ai/dsh-llm'
import { brandString } from '@deepseek-ai/dsh-brand'
import type { ToolCallId, ContentBlock, GenerateOptions, Message, StreamChunk } from '@deepseek-ai/dsh-llm'
import type { LlmPort, LlmRequest, LlmResponse, AgentToolCall } from './ports.ts'

/** The narrow slice of the harness LLM service the port needs. */
export interface LlmFacade {
  /**
   * Stream one model call.
   * @param options - the fully assembled request.
   * @returns the raw chunk stream.
   */
  stream(options: GenerateOptions): AsyncIterable<StreamChunk>
  /**
   * List provider routes with a registered adapter.
   * @returns provider metadata in registration order.
   */
  listProviders(): readonly { readonly id: string; readonly name: string }[]
  /**
   * List models one provider advertises.
   * @param provider - provider route id.
   * @returns the advertised models.
   */
  listModels(provider: string): Promise<readonly { readonly id: string; readonly name: string }[]>
}

/** Map runtime messages onto durable harness messages. */
function toHarnessMessages(request: LlmRequest): Message[] {
  const output: Message[] = []
  for (const message of request.messages) {
    if (message.role === 'user') {
      output.push(createUserMessage({ content: [{ type: 'text', text: message.text }], source: { kind: 'user' } }))
    } else if (message.role === 'tool') {
      output.push(createToolResultMessage({
        callId: brandString<ToolCallId>(message.toolCallId ?? ''),
        content: [{ type: 'text', text: message.text }],
        isError: message.isError === true,
      }))
    } else if (message.role === 'assistant') {
      output.push(createAssistantMessage({
        content: [{ type: 'text', text: message.text }],
        source: { provider: request.provider, model: request.model },
      }))
    }
  }
  return output
}

/**
 * Build the production model port over the harness LLM service.
 * @param llm - the LLM service slice.
 * @returns the port.
 */
export function createLlmPort(llm: LlmFacade): LlmPort {
  return {
    async complete(request: LlmRequest): Promise<LlmResponse> {
      const options: GenerateOptions = {
        provider: request.provider,
        model: request.model,
        system: request.system,
        messages: toHarnessMessages(request),
        tools: request.tools.map(tool => ({ name: tool.name, description: tool.description, parameters: tool.parameters })),
        maxTokens: request.maxTokens,
        signal: request.signal,
      }
      if (request.reasoningEffort !== undefined) {
        options.reasoningEffort = request.reasoningEffort as NonNullable<GenerateOptions['reasoningEffort']>
      }
      const textParts: string[] = []
      const toolCalls: AgentToolCall[] = []
      const blocks = new Map<number, Record<string, unknown>>()
      let usage: LlmResponse['usage'] = { inputTokens: 0, outputTokens: 0, cachedTokens: 0 }
      let stopReason = 'stop'
      for await (const chunk of llm.stream(options)) {
        if (chunk.type === 'text-delta') {
          textParts.push(chunk.text)
        } else if (chunk.type === 'tool-call-delta') {
          const entry = blocks.get(chunk.index) ?? { id: chunk.id, name: '', argumentsJson: '' }
          if (chunk.name !== undefined) entry.name = chunk.name
          entry.argumentsJson = String(entry.argumentsJson) + chunk.argumentsDelta
          blocks.set(chunk.index, entry)
        } else if (chunk.type === 'block-end') {
          const block: ContentBlock = chunk.block
          if (block.type === 'tool-call') {
            blocks.set(chunk.index, { id: block.id, name: block.name, argumentsJson: block.arguments })
          }
        } else if (chunk.type === 'usage') {
          usage = {
            inputTokens: chunk.usage.inputTokens,
            outputTokens: chunk.usage.outputTokens,
            cachedTokens: chunk.usage.cacheReadTokens ?? 0,
          }
        } else if (chunk.type === 'finish') {
          stopReason = chunk.reason.kind
        }
      }
      for (const entry of blocks.values()) {
        toolCalls.push({
          id: String(entry.id),
          name: String(entry.name),
          argumentsJson: String(entry.argumentsJson),
        })
      }
      return { text: textParts.join(''), toolCalls, usage, finishReason: stopReason }
    },
  }
}
