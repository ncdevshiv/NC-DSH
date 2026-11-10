/**
 * A scripted model port for integration and end-to-end tests.
 *
 * It is a real model port: the orchestrator drives its actual execution path,
 * the real tools run in real worktrees, and the scripted model replaces only
 * the network call to a provider. Scripts are keyed by a phrase in the agent
 * system prompt, so one test can drive several roles in one project.
 * @module @deepseek-ai/dsh-ncag-orchestrator/tests/scripted-model
 */

import type { AgentToolCall, LlmPort, LlmRequest, LlmResponse } from '@deepseek-ai/dsh-ncag-runtime'

/** One scripted step: a tool call, or the final answer. */
export type ScriptStep =
  | { readonly kind: 'tool'; readonly name: string; readonly args: Record<string, unknown> }
  | { readonly kind: 'final'; readonly text: string }

/** Script keys, matched against the agent system prompt. */
export const SCRIPT_KEYS = {
  architecture: 'You are the system architect',
  general: 'You are the general-purpose agent',
  backend: 'You are a backend engineer',
  reviewer: 'You are the code reviewer',
  critiquer: 'You are the critiquer',
} as const

/** A scripted model port that replays one script per model call. */
export class ScriptedModel implements LlmPort {
  private readonly scripts: Map<string, ScriptStep[]>
  private readonly calls: { readonly system: string; readonly messages: LlmRequest['messages'] }[]

  /**
   * @param scripts - script keyed by a phrase the agent system prompt contains.
   */
  constructor(scripts: ReadonlyMap<string, readonly ScriptStep[]>) {
    this.scripts = new Map()
    for (const [key, script] of scripts) this.scripts.set(key, [...script])
    this.calls = []
  }

  /** Model calls this port answered, in order. */
  get recorded(): readonly { readonly system: string; readonly messages: LlmRequest['messages'] }[] {
    return this.calls
  }

  /**
   * Append more scripted steps to one role.
   *
   * A test uses this when the number of runs for a role is not known before
   * the orchestrator starts, for example a successor agent that starts only
   * after another agent hands the task off.
   * @param key - system-prompt phrase selecting the role.
   * @param steps - the steps to append.
   */
  script(key: string, steps: readonly ScriptStep[]): void {
    this.scripts.set(key, [...(this.scripts.get(key) ?? []), ...steps])
  }

  /**
   * Answer one model call from the script selected by the system prompt.
   * @param request - the resolved request.
   * @returns the scripted response.
   */
  complete(request: LlmRequest): Promise<LlmResponse> {
    this.calls.push({ system: request.system, messages: request.messages })
    const key = this.selectKey(request.system)
    const script = this.scripts.get(key)
    if (script === undefined) {
      return Promise.reject(new Error(`scripted model has no script for ${JSON.stringify(key)}`))
    }
    const step = script.shift()
    if (step === undefined) {
      return Promise.resolve({ text: 'nothing further to do', toolCalls: [], usage: { inputTokens: 100, outputTokens: 20, cachedTokens: 0 }, finishReason: 'stop' })
    }
    if (step.kind === 'final') {
      return Promise.resolve({
        text: step.text,
        toolCalls: [],
        usage: { inputTokens: 100, outputTokens: 40, cachedTokens: 0 },
        finishReason: 'stop',
      })
    }
    const call: AgentToolCall = { id: `call_${this.calls.length}`, name: step.name, argumentsJson: JSON.stringify(step.args) }
    return Promise.resolve({
      text: '',
      toolCalls: [call],
      usage: { inputTokens: 100, outputTokens: 30, cachedTokens: 0 },
      finishReason: 'tool_calls',
    })
  }

  /** Choose the script for this request from the profile identity in the system prompt. */
  private selectKey(system: string): string {
    for (const [key] of this.scripts) {
      if (system.includes(key)) return key
    }
    return ''
  }
}
