/**
 * Prompt assembly for NC-AG agents: the stable system prompt built from the
 * profile contract plus the task and context package, and the initial user
 * message carrying the assignment.
 * @module @deepseek-ai/dsh-ncag-runtime/prompt
 */

import type { AgentProfile, ContextFact, Task } from '@deepseek-ai/dsh-ncag-core'
import type { AgentMessage } from './ports.ts'

/** Everything an agent receives when it starts one task. */
export interface ContextPackage {
  /** Project name. */
  readonly projectName: string
  /** Project-wide instructions. */
  readonly projectInstructions: string
  /** Approved architecture facts relevant to this task. */
  readonly architecture: readonly ContextFact[]
  /** Approved contracts relevant to this task. */
  readonly contracts: readonly ContextFact[]
  /** Constraints the agent must respect. */
  readonly constraints: readonly ContextFact[]
  /** Outputs produced by dependency tasks, keyed by task title. */
  readonly dependencyOutputs: readonly { readonly title: string; readonly summary: string }[]
  /** Messages addressed to this agent that it must act on. */
  readonly messages: readonly { readonly type: string; readonly subject: string; readonly content: string }[]
}

/** Render one context fact list as bullet lines. */
function bullets(title: string, facts: readonly ContextFact[]): string {
  if (facts.length === 0) return ''
  return `${title}\n${facts.map(fact => `- ${fact.text}`).join('\n')}\n`
}

/**
 * Build the complete system prompt for one assignment.
 * @param profile - the agent profile executing the task.
 * @param context - the context package for this task.
 * @returns the system prompt text.
 */
export function buildSystemPrompt(profile: AgentProfile, context: ContextPackage): string {
  const sections: string[] = []
  sections.push(profile.identity)
  sections.push(`Working style: ${profile.personality}`)
  sections.push(`Expertise: ${profile.expertise.join(', ')}`)
  sections.push(`Your responsibilities:\n${profile.responsibilities.map(item => `- ${item}`).join('\n')}`)
  sections.push(`Your restrictions:\n${profile.restrictions.map(item => `- ${item}`).join('\n')}`)
  sections.push(`Quality standard: ${profile.qualityStandard}`)
  sections.push(`Escalate instead of proceeding when:\n${profile.escalationRules.map(item => `- ${item}`).join('\n')}`)
  sections.push(`Project: ${context.projectName}`)
  if (context.projectInstructions.trim().length > 0) {
    sections.push(`Project instructions:\n${context.projectInstructions}`)
  }
  sections.push(bullets('Approved architecture:', context.architecture))
  sections.push(bullets('Approved contracts:', context.contracts))
  sections.push(bullets('Constraints:', context.constraints))
  sections.push('You work inside your isolated workspace. Every path you read or write is relative to it.'
 + ' Use the tools to inspect the real files before you change them; never assume a file contents.')
  sections.push('When you are done, reply with a short completion report naming what you changed, the commands you ran, and their exact outcomes.'
 + ' If you cannot finish, say exactly what blocks you.')
  return sections.filter(section => section.trim().length > 0).join('\n\n')
}

/**
 * Build the initial user message for one assignment.
 * @param task - the assignment.
 * @param context - the context package for this task.
 * @returns the user message.
 */
export function buildAssignmentMessage(task: Task, context: ContextPackage): AgentMessage {
  const parts: string[] = []
  parts.push(`Task: ${task.title}`)
  parts.push(task.description)
  if (task.requiredInputs.length > 0) {
    parts.push(`Required inputs:\n${task.requiredInputs.map(item => `- ${item}`).join('\n')}`)
  }
  if (task.expectedOutputs.length > 0) {
    parts.push(`Expected outputs:\n${task.expectedOutputs.map(item => `- ${item}`).join('\n')}`)
  }
  if (task.acceptanceCriteria.length > 0) {
    parts.push(`Acceptance criteria (each must end verified):\n${task.acceptanceCriteria.map(criterion => `- ${criterion.text}`).join('\n')}`)
  }
  if (context.dependencyOutputs.length > 0) {
    parts.push(`Completed dependency work:\n${context.dependencyOutputs.map(item => `- ${item.title}: ${item.summary}`).join('\n')}`)
  }
  if (context.messages.length > 0) {
    parts.push(`Relevant coordination messages:\n${context.messages.map(item => `- [${item.type}] ${item.subject}: ${item.content}`).join('\n')}`)
  }
  return { role: 'user', text: parts.join('\n\n') }
}
