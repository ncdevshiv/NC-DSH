/**
 * Remote failure vocabulary of the `ncag` namespace. Host-face only: the
 * browser reads failures through the protocol's own code union, so the wire
 * types stay free of any protocol module edge.
 * @module @deepseek-ai/dsh-api-ncag-controller/codes
 */

declare module '@deepseek-ai/dsh-typert-protocol' {
  interface RemoteErrorDetailsMap {
    /** The addressed project, task, agent, or decision no longer exists. */
    'ncag/not-found': {
      readonly projectId?: string
      readonly taskId?: string
      readonly agentId?: string
      readonly decisionId?: string
    }
    /** The orchestrator rejected the operation's arguments or its current state. */
    'ncag/invalid': { readonly operation: string }
  }
}

export {}
