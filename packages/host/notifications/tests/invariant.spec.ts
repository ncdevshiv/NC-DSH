import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import { InvariantError, InvariantRegistry } from '@deepseek-ai/dsh-invariants'
import NotificationsService from '../src/index.ts'
import * as NotificationsInvariant from '../src/invariant.ts'

const roots: string[] = []
const contexts: Context[] = []

afterEach(async () => {
  await Promise.all(contexts.splice(0).map(ctx => ctx.fiber.dispose()))
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

/** Boot the registry, the companion, and one real service over a fresh home. */
async function setup(): Promise<{ ctx: Context; service: NotificationsService }> {
  const ctx = new Context()
  contexts.push(ctx)
  const home = mkdtempSync(join(tmpdir(), 'dsh-notifications-invariant-'))
  roots.push(home)
  await ctx.plugin(InvariantRegistry)
  await ctx.plugin(NotificationsInvariant)
  await ctx.plugin(NotificationsService, { dshHome: home })
  return { ctx, service: ctx.notifications }
}

describe('notifications invariants', () => {
  it('accepts a committed update whose id is held by the service', async () => {
    const { service } = await setup()
    expect(() => service.publish({ id: 'a', kind: 'k', title: 't' })).not.toThrow()
    expect(() => service.dismiss('a')).not.toThrow()
  })

  it('accepts a committed removal whose id is absent from the service', async () => {
    const { service } = await setup()
    service.publish({ id: 'a', kind: 'k', title: 't' })
    expect(() => service.delete('a')).not.toThrow()
  })

  it('fails an updated emission for an id the service does not hold', async () => {
    const { ctx } = await setup()
    expect(() => { ctx.emit('notifications/updated', 'ghost') })
      .toThrow(InvariantError)
    try {
      ctx.emit('notifications/updated', 'ghost')
    } catch (error) {
      expect((error as InvariantError).message).toMatch(/does not hold/)
    }
  })

  it('fails a removed emission for an id the service still holds', async () => {
    const { ctx, service } = await setup()
    service.publish({ id: 'kept', kind: 'k', title: 't' })
    expect(() => { ctx.emit('notifications/removed', 'kept') })
      .toThrow(/still holds/)
  })
})
