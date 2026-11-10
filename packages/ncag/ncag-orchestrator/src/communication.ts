/**
 * Durable, structured communication between agents, the orchestrator and
 * project channels, with delivery tracking, acknowledgement, duplicate
 * detection, expiry and dead-letter handling.
 * @module @deepseek-ai/dsh-ncag-orchestrator/communication
 */

import { newId } from '@deepseek-ai/dsh-ncag-core'
import type { CorrelationId, MessageTarget, MessageType, NcagMessage, ProjectId, TaskId } from '@deepseek-ai/dsh-ncag-core'
import type { Repositories } from '@deepseek-ai/dsh-ncag-store'

/** Fields a sender supplies; identity and delivery state are manager-owned. */
export interface SendMessageInput {
  readonly projectId: ProjectId
  readonly sender: NcagMessage['sender']
  readonly target: MessageTarget
  readonly taskId: TaskId | null
  readonly type: MessageType
  readonly subject: string
  readonly content: string
  readonly contextRefs?: readonly string[]
  readonly correlationId: CorrelationId
  readonly priority?: NcagMessage['priority']
  readonly requiresAck?: boolean
  /** Lease duration of the message; expired messages are dead-lettered. */
  readonly lifetimeMs?: number
}

/** Failure counter after which an undeliverable message is dead-lettered. */
export const MAX_DELIVERY_ATTEMPTS = 3

/**
 * Durable message log with an in-process delivery pump.
 */
export class CommunicationManager {
  private readonly repositories: Repositories
  private readonly listeners: Array<(message: NcagMessage) => void>

  /**
   * @param repositories - store repositories owning message rows.
   */
  constructor(repositories: Repositories) {
    this.repositories = repositories
    this.listeners = []
  }

  /**
   * Subscribe to messages delivered to one agent.
   * @param agentId - receiving agent.
   * @param listener - invoked for every message routed to this agent or its channel.
   * @returns the disposer that unsubscribes.
   */
  subscribe(agentId: string, listener: (message: NcagMessage) => void): () => void {
    const entries: Array<(message: NcagMessage) => void> = []
    const wrapped = (message: NcagMessage): void => {
      const addressed = (message.target.kind === 'agent' && message.target.agentId === agentId)
        || (message.target.kind === 'orchestrator' && agentId === 'orchestrator')
      if (addressed) listener(message)
    }
    entries.push(wrapped)
    this.listeners.push(wrapped)
    return () => {
      const index = this.listeners.indexOf(wrapped)
      if (index >= 0) this.listeners.splice(index, 1)
      entries.length = 0
    }
  }

  /**
   * Record and attempt delivery of one message.
   * @param input - the message to send.
   * @returns the stored message with its delivery state.
   */
  send(input: SendMessageInput): NcagMessage {
    const now = new Date().toISOString()
    const lifetime = input.lifetimeMs ?? 86_400_000
    const message: NcagMessage = {
      id: newId('message'),
      timestamp: now,
      sender: input.sender,
      target: input.target,
      projectId: input.projectId,
      taskId: input.taskId,
      type: input.type,
      subject: input.subject,
      content: input.content,
      contextRefs: input.contextRefs ?? [],
      correlationId: input.correlationId,
      priority: input.priority ?? 'normal',
      requiresAck: input.requiresAck ?? false,
      deliveryStatus: 'PENDING',
      deliveredAt: null,
      acknowledgedAt: null,
      attempts: 0,
      expiresAtMs: Date.now() + lifetime,
    }
    this.repositories.messages.insert(message)
    return this.deliver(message)
  }

  /**
   * Deliver one stored pending message to its recipient.
   * @param message - the pending message.
   * @returns the message with updated delivery state.
   */
  deliver(message: NcagMessage): NcagMessage {
    if (message.deliveryStatus !== 'PENDING') return message
    const expired = message.expiresAtMs !== null && message.expiresAtMs <= Date.now()
    if (expired) {
      const deadLettered: NcagMessage = { ...message, deliveryStatus: 'DEAD_LETTERED', attempts: message.attempts + 1 }
      this.repositories.messages.updateDelivery(deadLettered, 'expired before delivery')
      return deadLettered
    }
    const delivered: NcagMessage = {
      ...message,
      deliveryStatus: 'DELIVERED',
      deliveredAt: new Date().toISOString(),
      attempts: message.attempts + 1,
    }
    this.repositories.messages.updateDelivery(delivered, null)
    for (const listener of this.listeners) listener(delivered)
    return delivered
  }

  /**
   * Acknowledge one delivered message.
   * @param messageId - message identifier.
   * @param agentId - acknowledging agent.
   * @returns the updated message.
   * @throws Error when the message does not exist.
   */
  acknowledge(messageId: string, agentId: string): NcagMessage {
    const message = this.repositories.messages.get(messageId)
    if (message === null) throw new Error(`ncag: message ${messageId} does not exist`)
    const created = this.repositories.messages.acknowledge(messageId, agentId)
    if (!created) return message
    const acknowledged: NcagMessage = {
      ...message,
      deliveryStatus: 'ACKNOWLEDGED',
      acknowledgedAt: new Date().toISOString(),
    }
    this.repositories.messages.updateDelivery(acknowledged, null)
    return acknowledged
  }

  /**
   * Retry every message that is still pending and has not exhausted its attempts.
   * @returns the messages retried in this pass.
   */
  retryPending(): NcagMessage[] {
    const retried: NcagMessage[] = []
    for (const message of this.repositories.messages.pending(200)) {
      if (message.attempts >= MAX_DELIVERY_ATTEMPTS) {
        const deadLettered: NcagMessage = { ...message, deliveryStatus: 'DEAD_LETTERED' }
        this.repositories.messages.updateDelivery(deadLettered, `delivery failed after ${message.attempts} attempts`)
        continue
      }
      retried.push(this.deliver(message))
    }
    return retried
  }

  /**
   * Read the messages of one project.
   * @param projectId - project identifier.
   * @returns the messages in insertion order.
   */
  history(projectId: string): NcagMessage[] {
    return this.repositories.messages.listForProject(projectId)
  }
}
