/**
 * The agent turn loop: it asks the model for the next action, executes the
 * tools the model requests, feeds the results back, and stops when the model
 * produces a final answer or the step budget is exhausted. Every step, tool
 * call and usage measurement is reported through the callbacks so the
 * orchestrator can persist and monitor it.
 * @module @deepseek-ai/dsh-ncag-runtime/loop
 */

import type { AgentCoordination, AgentMessage, AgentToolCall, AgentToolSchema, LlmPort, LlmRequest, LlmUsage, ToolEnvironment } from './ports.ts'
import type { RuntimeTool } from './tools.ts'

/** Callbacks the orchestrator supplies for one agent run. */
export interface LoopHooks {
  /** Invoked before every model request with the exact request about to be sent. */
  readonly beforeRequest: (request: LlmRequest, step: number) => void
  /** Invoked after every completed model request. */
  readonly afterResponse: (usage: LlmUsage, responseText: string, step: number) => void
  /** Invoked around every tool execution. */
  readonly onToolCall: (call: AgentToolCall, args: unknown) => void
  /** Invoked after every tool execution with its outcome and duration. */
  readonly onToolResult: (call: AgentToolCall, outcome: { text: string; isError: boolean }, durationMs: number) => void
}

/** Request to run one agent to completion of its task. */
export interface AgentRunRequest {
  /** Agent identity executing the run. */
  readonly agentId: string
  /** Task the agent works on, when assigned. */
  readonly taskId: string | null
  /** Project the agent belongs to. */
  readonly projectId: string
  /** Absolute workspace root. */
  readonly workspacePath: string
  /** Resolved model route. */
  readonly provider: string
  readonly model: string
  readonly reasoningEffort?: string
  /** Complete system prompt for this agent. */
  readonly system: string
  /** Initial user messages describing the assignment. */
  readonly messages: readonly AgentMessage[]
  /** Tools the agent may use. */
  readonly tools: readonly RuntimeTool[]
  /** Maximum model steps before the loop stops. */
  readonly maxSteps: number
  /** Output token ceiling per model call. */
  readonly maxTokens: number
  /** Cancellation signal. */
  readonly signal: AbortSignal
  /** Coordination surface the agent's help and handoff tools call, when an orchestrator drives the run. */
  readonly coordination?: AgentCoordination
  /** Observation callbacks. */
  readonly hooks: LoopHooks
}

/** Final state of one agent run. */
export interface AgentRunResult {
  /** Final assistant text. */
  readonly finalText: string
  /** Whether the loop ended without exhausting the step budget. */
  readonly completed: boolean
  /** Steps executed. */
  readonly steps: number
  /** Accumulated usage. */
  readonly usage: LlmUsage
  /** Complete conversation the run produced. */
  readonly transcript: readonly AgentMessage[]
}

/** Parse tool arguments, reporting a malformed payload as an executor-visible failure. */
function parseArguments(text: string): { ok: true; value: unknown } | { ok: false; error: string } {
  try {
    return { ok: true, value: JSON.parse(text) as unknown }
  } catch (error: unknown) {
    return { ok: false, error: (error as Error).message }
  }
}

/** Validate the arguments of one call against the tool schema's required keys. */
function validateRequired(schema: AgentToolSchema, value: unknown): string | null {
  const required = (schema.parameters.required ?? []) as string[]
  if (value === null || typeof value !== 'object') return 'arguments must be a JSON object'
  const record = value as Record<string, unknown>
  for (const key of required) {
    if (!Object.hasOwn(record, key)) return `missing required argument ${JSON.stringify(key)}`
  }
  return null
}

/**
 * Run one agent to completion.
 * @param port - model execution port.
 * @param request - the run request.
 * @returns the final state of the run.
 */
export async function runAgent(port: LlmPort, request: AgentRunRequest): Promise<AgentRunResult> {
  const transcript: AgentMessage[] = [...request.messages]
  const toolSchemas: AgentToolSchema[] = request.tools.map(tool => tool.schema)
  const usage: LlmUsage = { inputTokens: 0, outputTokens: 0, cachedTokens: 0 }
  let steps = 0
  let finalText = ''
  while (steps < request.maxSteps) {
    if (request.signal.aborted) throw new Error('ncag: agent run aborted')
    steps += 1
    const call: LlmRequest = {
      provider: request.provider,
      model: request.model,
      reasoningEffort: request.reasoningEffort,
      system: request.system,
      messages: [...transcript],
      tools: toolSchemas,
      maxTokens: request.maxTokens,
      signal: request.signal,
    }
    request.hooks.beforeRequest(call, steps)
    const response = await port.complete(call)
    usage.inputTokens += response.usage.inputTokens
    usage.outputTokens += response.usage.outputTokens
    usage.cachedTokens += response.usage.cachedTokens
    request.hooks.afterResponse(response.usage, response.text, steps)
    if (response.text.trim().length > 0) {
      finalText = response.text.trim()
      transcript.push({ role: 'assistant', text: finalText })
    }
    if (response.toolCalls.length === 0) {
      return { finalText, completed: true, steps, usage, transcript }
    }
    for (const toolCall of response.toolCalls) {
      const started = Date.now()
      const parsed = parseArguments(toolCall.argumentsJson)
      const tool = request.tools.find(candidate => candidate.schema.name === toolCall.name)
      let outcome: { text: string; isError: boolean }
      if (!parsed.ok) {
        outcome = { text: `invalid tool arguments: ${parsed.error}`, isError: true }
      } else if (tool === undefined) {
        outcome = { text: `unknown tool ${JSON.stringify(toolCall.name)}`, isError: true }
      } else {
        const invalid = validateRequired(tool.schema, parsed.value)
        if (invalid !== null) {
          outcome = { text: `invalid tool arguments: ${invalid}`, isError: true }
        } else {
          request.hooks.onToolCall(toolCall, parsed.value)
          const environment: ToolEnvironment = {
            workspacePath: request.workspacePath,
            agentId: request.agentId,
            taskId: request.taskId,
            projectId: request.projectId,
            signal: request.signal,
            ...(request.coordination === undefined ? {} : { coordination: request.coordination }),
          }
          // A coordination tool waits for the orchestrator, so the call is
          // awaited; both a direct outcome and a promise settle here.
          try {
            outcome = await tool.execute(parsed.value, environment)
          } catch (error: unknown) {
            outcome = { text: `tool failed: ${(error as Error).message}`, isError: true }
          }
        }
      }
      const durationMs = Date.now() - started
      request.hooks.onToolResult(toolCall, outcome, durationMs)
      transcript.push({
        role: 'tool',
        text: outcome.text,
        toolCallId: toolCall.id,
        toolName: toolCall.name,
        isError: outcome.isError,
      })
    }
  }
  return { finalText, completed: false, steps, usage, transcript }
}
