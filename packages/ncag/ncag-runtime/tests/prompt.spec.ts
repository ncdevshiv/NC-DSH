import { describe, expect, it } from 'vitest'
import { buildAssignmentMessage, buildSystemPrompt } from '../src/prompt.ts'
import type { ContextPackage } from '../src/prompt.ts'
import { BUILT_IN_PROFILES, builtInProfile, newId } from '@deepseek-ai/dsh-ncag-core'
import type { AgentProfile, Task } from '@deepseek-ai/dsh-ncag-core'

const seed = BUILT_IN_PROFILES.find(profile => profile.key === 'general-purpose')
if (seed === undefined) throw new Error('missing general-purpose seed')
const profile: AgentProfile = builtInProfile(seed)

function context(overrides: Partial<ContextPackage> = {}): ContextPackage {
  return {
    projectName: 'demo',
    projectInstructions: '',
    architecture: [],
    contracts: [],
    constraints: [],
    dependencyOutputs: [],
    messages: [],
    ...overrides,
  }
}

function task(overrides: Partial<Task> = {}): Task {
  const now = new Date().toISOString()
  return {
    id: newId('task'),
    projectId: newId('project'),
    requestId: null,
    requirementIds: [],
    title: 'Implement it',
    description: 'Do the thing',
    requiredCapability: 'general-implementation',
    assignedAgentId: null,
    dependencyIds: [],
    requiredInputs: [],
    expectedOutputs: [],
    acceptanceCriteria: [],
    requiredPermissions: [],
    contextRefs: [],
    priority: 'P1',
    deadlineMs: null,
    requiredReviews: [],
    requiredVerifications: [],
    status: 'PLANNED',
    depth: 0,
    createdAt: now,
    updatedAt: now,
    startedAt: null,
    completedAt: null,
    attempts: 0,
    reviewCycles: 0,
    consultation: false,
    statusReason: null,
    ...overrides,
  }
}

describe('ncag prompt assembly', () => {
  it('omits empty context sections from the system prompt', () => {
    const prompt = buildSystemPrompt(profile, context())
    expect(prompt).toContain(profile.identity)
    expect(prompt).toContain('Project: demo')
    expect(prompt).not.toContain('Project instructions:')
    expect(prompt).not.toContain('Approved architecture:')
    expect(prompt).not.toContain('Approved contracts:')
    expect(prompt).not.toContain('Constraints:')
    expect(prompt).toContain('isolated workspace')
  })

  it('includes project instructions and context facts when present', () => {
    const prompt = buildSystemPrompt(profile, context({
      projectInstructions: 'Use TypeScript.',
      architecture: [{ id: 'a1', text: 'Modules are isolated', source: 'decision', recordedAt: 'now' }],
      contracts: [{ id: 'c1', text: 'Return JSON', source: 'decision', recordedAt: 'now' }],
      constraints: [{ id: 'k1', text: 'No new dependencies', source: 'decision', recordedAt: 'now' }],
    }))
    expect(prompt).toContain('Project instructions:\nUse TypeScript.')
    expect(prompt).toContain('Approved architecture:\n- Modules are isolated')
    expect(prompt).toContain('Approved contracts:\n- Return JSON')
    expect(prompt).toContain('Constraints:\n- No new dependencies')
  })

  it('builds an assignment message with only the task text when nothing else is supplied', () => {
    const message = buildAssignmentMessage(task(), context())
    expect(message.role).toBe('user')
    expect(message.text).toBe('Task: Implement it\n\nDo the thing')
  })

  it('includes inputs, outputs, criteria, dependency work and messages when present', () => {
    const message = buildAssignmentMessage(task({
      requiredInputs: ['the contract'],
      expectedOutputs: ['the module'],
      acceptanceCriteria: [{ id: 'ac-1', text: 'tests pass', verification: 'unit-test' }],
    }), context({
      dependencyOutputs: [{ title: 'Schema', summary: 'tables created' }],
      messages: [{ type: 'INFORMATION', subject: 'note', content: 'read the spec' }],
    }))
    expect(message.text).toContain('Required inputs:\n- the contract')
    expect(message.text).toContain('Expected outputs:\n- the module')
    expect(message.text).toContain('Acceptance criteria (each must end verified):\n- tests pass')
    expect(message.text).toContain('Completed dependency work:\n- Schema: tables created')
    expect(message.text).toContain('Relevant coordination messages:\n- [INFORMATION] note: read the spec')
  })
})
