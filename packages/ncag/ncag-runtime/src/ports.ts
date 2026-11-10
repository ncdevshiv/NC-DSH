/**
 * The model and tool interfaces an NC-AG agent executes against. The runtime
 * depends on these interfaces rather than on a provider or a filesystem
 * backend, so the same agent loop drives the production provider route and
 * the tests.
 * @module @deepseek-ai/dsh-ncag-runtime/ports
 */

import type { ModelTier } from '@deepseek-ai/dsh-ncag-core'

/** One conversation message in the runtime's provider-neutral form. */
export interface AgentMessage {
  /** Message role. */
  readonly role: 'system' | 'user' | 'assistant' | 'tool'
  /** Message text. */
  readonly text: string
  /** Tool call answered by a tool-role message. */
  readonly toolCallId?: string
  /** Tool name answered by a tool-role message. */
  readonly toolName?: string
  /** Whether the answered tool call failed. */
  readonly isError?: boolean
}

/** One tool the model may call, in JSON Schema form. */
export interface AgentToolSchema {
  readonly name: string
  readonly description: string
  readonly parameters: Record<string, unknown>
}

/** One tool call the model requested. */
export interface AgentToolCall {
  /** Provider call id. */
  readonly id: string
  /** Tool name. */
  readonly name: string
  /** Raw JSON argument text as the model produced it. */
  readonly argumentsJson: string
}

/** Token usage reported by one model call. */
export interface LlmUsage {
  inputTokens: number
  outputTokens: number
  cachedTokens: number
}

/** One resolved model call request. */
export interface LlmRequest {
  /** Registered provider route. */
  readonly provider: string
  /** Provider model id. */
  readonly model: string
  /** Optional reasoning effort identifier. */
  readonly reasoningEffort?: string | undefined
  /** System prompt text. */
  readonly system: string
  /** Conversation messages, excluding the system prompt. */
  readonly messages: readonly AgentMessage[]
  /** Tool declarations. */
  readonly tools: readonly AgentToolSchema[]
  /** Output token ceiling for this call. */
  readonly maxTokens: number
  /** Cancellation signal forwarded to the provider. */
  readonly signal: AbortSignal
}

/** One model call result. */
export interface LlmResponse {
  /** Visible assistant text. */
  readonly text: string
  /** Tool calls the model requested. */
  readonly toolCalls: readonly AgentToolCall[]
  /** Reported usage. */
  readonly usage: LlmUsage
  /** Provider stop reason name, for diagnostics. */
  readonly finishReason: string
}

/**
 * Model execution port. The production implementation routes through the
 * harness LLM service; tests supply a scripted implementation.
 */
export interface LlmPort {
  /**
   * Execute one model call and settle when the response is complete.
   * @param request - the fully resolved request.
   * @returns the complete response.
   */
  complete(request: LlmRequest): Promise<LlmResponse>
}

/** A model route the deployment can serve. */
export interface ModelRouteInfo {
  readonly provider: string
  readonly model: string
  /** Tier this route serves. */
  readonly tier: ModelTier
  /** Relative cost weight used by the economical policy. */
  readonly costWeight: number
}

/** One help request a working agent raises through the runtime tool. */
export interface HelpRequest {
  /** What the agent cannot resolve on its own. */
  readonly problem: string
  /** Capability of the specialist the agent wants consulted; empty asks the orchestrator to choose. */
  readonly capability: string
  readonly urgency: 'low' | 'normal' | 'high' | 'critical'
  /** Evidence the helper needs to answer. */
  readonly context: string
  /** What the requesting agent expects the helper to produce. */
  readonly expectedAssistance: string
  /** Whether a new specialist instance is required rather than an existing one. */
  readonly newHelperRequired: boolean
}

/** One handoff package a working agent raises through the runtime tool. */
export interface HandoffRequest {
  /** Capability of the agent that should continue the work. */
  readonly capability: string
  readonly completedWork: string
  readonly remainingWork: string
  readonly knownProblems: string
  readonly outstandingQuestions: string
}

/**
 * Coordination surface the orchestrator exposes to an agent's tools.
 *
 * The runtime stays provider- and platform-neutral: a tool asks through this
 * port, and the orchestrator answers with a real helper run or a recorded
 * handoff. Absent coordination, the tool reports that no orchestrator is
 * attached instead of pretending the request was served.
 */
export interface AgentCoordination {
  /**
   * Ask the platform for help on the current task.
   * @param request - the help request.
   * @returns the helper's answer as a tool outcome.
   */
  requestHelp(request: HelpRequest): Promise<ToolOutcome>
  /**
   * Hand the current task to another agent.
   * @param request - the handoff package.
   * @returns whether the platform accepted and recorded the handoff.
   */
  handOff(request: HandoffRequest): Promise<ToolOutcome>
}

/** Execution environment a tool receives. */
export interface ToolEnvironment {
  /** Absolute workspace root the tool may read. */
  readonly workspacePath: string
  /** Agent executing the tool. */
  readonly agentId: string
  /** Task the agent is working on. */
  readonly taskId: string | null
  /** Project the agent belongs to. */
  readonly projectId: string
  /** Cancellation signal bounding the call. */
  readonly signal: AbortSignal
  /** Coordination surface, present when an orchestrator drives the run. */
  readonly coordination?: AgentCoordination
}

/** One tool execution outcome. */
export interface ToolOutcome {
  /** Model-facing result text. */
  readonly text: string
  /** Whether the call failed. */
  readonly isError: boolean
}

