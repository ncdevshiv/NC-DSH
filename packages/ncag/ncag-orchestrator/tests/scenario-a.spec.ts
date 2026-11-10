import { afterEach, describe, expect, it } from 'vitest'
import { SCRIPT_KEYS } from './scripted-model.ts'
import type { ScriptStep } from './scripted-model.ts'
import type { OrchestratorConfig } from '../src/index.ts'
import { startHarness } from './harness.ts'
import type { Harness } from './harness.ts'

const harnesses: Harness[] = []

afterEach(async () => {
  while (harnesses.length > 0) {
    const harness = harnesses.pop()
    if (harness !== undefined) await harness.cleanup()
  }
})

async function harness(
  scripts: readonly (readonly [string, readonly ScriptStep[]])[],
  overrides: Partial<OrchestratorConfig> = {},
): Promise<Harness> {
  const created = await startHarness(new Map(scripts), overrides)
  harnesses.push(created)
  return created
}

describe('ncag end-to-end scenarios', () => {
  it('A: delivers a simple request through plan, execution, review and verification', async () => {
    const h = await harness([
      [SCRIPT_KEYS.general, [
        { kind: 'tool', name: 'write', args: { path: 'feature.txt', content: 'delivered feature\n' } },
        { kind: 'tool', name: 'shell', args: { command: 'Get-Content feature.txt' } },
        { kind: 'final', text: 'wrote feature.txt and read it back' },
      ]],
    ])
    const project = h.orchestrator.createProject({
      name: 'simple',
      description: 'simple feature',
      repositoryPath: h.repository,
      checks: {
        'unit-test': 'if (-not (Test-Path feature.txt)) { exit 1 }',
        'manual-review': 'Get-Content README.md',
      },
    })
    const result = await h.orchestrator.submitRequest({ projectId: project.id, rawText: 'Implement a small feature.' })
    expect(result.tasks.length).toBeGreaterThan(0)
    expect(result.plan.intent.goal.length).toBeGreaterThan(0)
    const firstTaskId = result.tasks[0]?.id ?? ''
    const task = h.orchestrator.repositories.tasks.get(firstTaskId)
    const evidence = h.orchestrator.repositories.evidence.listForTask(firstTaskId)
    if (task?.status !== 'COMPLETED') {
      throw new Error(`status=${task?.status} reason=${task?.statusReason} evidence=${JSON.stringify(evidence.map(entry => ({ id: entry.criterionId, status: entry.status, command: entry.command, output: entry.output.slice(0, 300) })))}`)
    }
    expect(task?.status).toBe('COMPLETED')
    expect(evidence.length).toBeGreaterThan(0)
    expect(evidence.every(entry => entry.status === 'PASSED')).toBe(true)
    expect(h.orchestrator.store.verifyAudit().brokenAt).toBeNull()
  }, 60_000)
})
