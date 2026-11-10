/**
 * Delegation to a model for the analyses NC-AG performs on its own work:
 * plan critique, review, audit, and conflict analysis.
 *
 * A delegation is one model call with no tools. Its answer is parsed and
 * validated by the caller; a malformed answer is a failure, never a
 * silently-accepted result.
 * @module @deepseek-ai/dsh-ncag-orchestrator/delegate
 */

import type { LlmPort } from '@deepseek-ai/dsh-ncag-runtime'
import type { ModelRoute } from '@deepseek-ai/dsh-ncag-core'

/** One delegated analysis request. */
export interface DelegateRequest {
  /** Analysis purpose, recorded in the audit trail. */
  readonly purpose: string
  /** System prompt describing the analysis contract. */
  readonly system: string
  /** The material to analyze. */
  readonly prompt: string
  /** Output token ceiling. */
  readonly maxTokens: number
  /** Cancellation signal. */
  readonly signal: AbortSignal
}

/** The delegation port the orchestrator calls. */
export interface DelegationPort {
  /** Whether this port can answer at all; false when no route is configured. */
  readonly available: boolean
  /**
   * Ask the delegate one question.
   * @param request - the analysis request.
   * @returns the delegate's complete answer text.
   */
  ask(request: DelegateRequest): Promise<string>
}

/** A port that refuses every call, used when the deployment configured no route. */
export class UnavailableDelegation implements DelegationPort {
  /** Always false. */
  readonly available: false = false

  /**
   * @param reason - why delegation is unavailable, reported to the caller.
   */
  private readonly reason: string

  /**
   * @param reason - why delegation is unavailable, reported to the caller.
   */
  constructor(reason: string) {
    this.reason = reason
  }

  /**
   * Refuse the call.
   * @param request - the analysis request that could not run.
   * @returns never returns; the promise rejects.
   */
  ask(request: DelegateRequest): Promise<string> {
    return Promise.reject(new Error(`ncag: delegated ${request.purpose} analysis is unavailable: ${this.reason}`))
  }
}

/**
 * Build a delegation port over one resolved model route.
 * @param llm - the model port.
 * @param route - resolved route used for every delegated analysis.
 * @returns the port.
 */
export function createDelegation(llm: LlmPort, route: ModelRoute): DelegationPort {
  return {
    available: true,
    async ask(request: DelegateRequest): Promise<string> {
      const response = await llm.complete({
        provider: route.provider,
        model: route.model,
        system: request.system,
        messages: [{ role: 'user', text: request.prompt }],
        tools: [],
        maxTokens: request.maxTokens,
        signal: request.signal,
      })
      return response.text.trim()
    },
  }
}
