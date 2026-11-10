import { describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import { mountAgentLoopTestDependencies } from '@deepseek-ai/dsh-agent-loop-testkit'
import SubagentRuntime from '@deepseek-ai/dsh-subagent'
import { mountWorkingDirectoryFixture } from '../../subagent/tests/working-directory-fixture.ts'
import { DELEGATION_TOOL_TABLE } from '../src/delegation-tool-table.ts'
import * as tool from '../src/index.ts'
import { ownContext } from './harness.ts'
import * as mock from './scripted-provider.ts'

/** Read the model-visible delegation paragraphs, excluding empty sections. */
async function delegationGuidance(ctx: Context): Promise<string[]> {
  const assembly = await ctx.systemPrompt.assemble()
  return assembly.sections.map(section => section.text).filter(text => text.startsWith('Start independent delegations'))
}

describe('delegation tool table ownership', () => {
  it('shares one composition-owned table and withdraws it with the composition', async () => {
    const ctx = ownContext(new Context())
    await mountAgentLoopTestDependencies(ctx)
    await mountWorkingDirectoryFixture(ctx)
    await ctx.plugin(SubagentRuntime)
    await mock.mountScriptedProvider(ctx, { name: 'fork' })
    await mock.mountScriptedProvider(ctx, { name: 'spawn' })

    const forkTool = await ctx.plugin(tool, { provider: 'fork', toolName: 'subagent_fork' })
    const spawnTool = await ctx.plugin(tool, { provider: 'spawn' })

    // One service instance holds both instances' definitions.
    const table = ctx.get(DELEGATION_TOOL_TABLE)
    expect(table).toBeDefined()
    expect(table?.list().map(definition => definition.name).sort()).toEqual(['subagent', 'subagent_fork'])
    expect(await delegationGuidance(ctx)).toEqual([
      'Start independent delegations with `subagent` or `subagent_fork` together in one assistant message and continue useful work while they run.',
    ])

    // Each unload withdraws that instance's entry; the last one empties the table.
    await forkTool.dispose()
    expect(table?.list().map(definition => definition.name)).toEqual(['subagent'])
    await spawnTool.dispose()
    expect(table?.list()).toEqual([])

    // The composition owner withdraws the service with itself.
    await ctx.fiber.dispose()
    expect(ctx.get(DELEGATION_TOOL_TABLE)).toBeUndefined()
  })
})
