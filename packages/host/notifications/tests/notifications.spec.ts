import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import NotificationsService from '../src/index.ts'
import type { NotificationView } from '../src/types.ts'
import { parseStore, renderStore, writeFileAtomicSync } from '../src/persist.ts'

const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

/** Create one fresh harness home per test. */
function freshHome(): string {
  const root = mkdtempSync(join(tmpdir(), 'dsh-notifications-'))
  roots.push(root)
  return root
}

/** Boot one service over a fresh home with its owning context. */
function boot(home = freshHome()): { ctx: Context; service: NotificationsService } {
  const ctx = new Context()
  const service = new NotificationsService(ctx, { dshHome: home })
  return { ctx, service }
}

describe('publish, replace, and list ordering', () => {
  it('lists entries newest first in reverse insertion order', () => {
    const { service } = boot()
    service.publish({ id: 'a', kind: 'k', title: 'first' })
    service.publish({ id: 'b', kind: 'k', title: 'second' })
    expect(service.list().map(view => view.id)).toEqual(['b', 'a'])
  })

  it('replaces content of an existing id, keeping createdAt and resetting flags', () => {
    const { service } = boot()
    service.publish({ id: 'a', kind: 'old-kind', title: 'old', body: 'old body' })
    service.setRead('a')
    service.dismiss('a')
    const createdBefore = service.list()[0]?.createdAt
    expect(createdBefore).toEqual(expect.any(String))
    service.publish({ id: 'a', kind: 'new-kind', title: 'new', data: { n: 1 } })
    const view = service.list()[0]
    expect(view).toMatchObject({
      id: 'a',
      kind: 'new-kind',
      title: 'new',
      data: { n: 1 },
      read: false,
      dismissed: false,
    })
    expect(view?.createdAt).toBe(createdBefore)
    expect(view?.body).toBeUndefined()
  })

  it('hands out frozen views that do not alias caller or store data', () => {
    const { service } = boot()
    const data = { n: 1 }
    service.publish({ id: 'a', kind: 'k', title: 't', data })
    data.n = 999
    const view = service.list()[0] as NotificationView & { data?: Record<string, unknown> }
    expect(view.data?.['n']).toBe(1)
    expect(Object.isFrozen(view)).toBe(true)
  })
})

describe('read, dismiss, and delete', () => {
  it('setRead defaults to true, supports false, and rejects unknown ids loud', () => {
    const { service } = boot()
    service.publish({ id: 'a', kind: 'k', title: 't' })
    service.setRead('a')
    expect(service.list()[0]?.read).toBe(true)
    service.setRead('a', false)
    expect(service.list()[0]?.read).toBe(false)
    expect(() => service.setRead('ghost')).toThrow(/unknown id "ghost"/)
  })

  it('dismiss marks without deleting and ignores a second dismiss', () => {
    const { service } = boot()
    service.publish({ id: 'a', kind: 'k', title: 't' })
    service.dismiss('a')
    service.dismiss('a')
    expect(service.list()[0]).toMatchObject({ dismissed: true })
    expect(service.list()).toHaveLength(1)
    expect(() => service.dismiss('ghost')).toThrow(/unknown id/)
  })

  it('delete removes the entry; deleting an absent id is satisfied', () => {
    const { service } = boot()
    service.publish({ id: 'a', kind: 'k', title: 't' })
    service.delete('a')
    service.delete('a')
    expect(service.list()).toHaveLength(0)
  })
})

describe('events', () => {
  it('emits updated on publish/replace/setRead/dismiss and removed on delete', () => {
    const { ctx, service } = boot()
    const updates: string[] = []
    const removals: string[] = []
    ctx.on('notifications/updated', (id) => { updates.push(id) })
    ctx.on('notifications/removed', (id) => { removals.push(id) })

    service.publish({ id: 'a', kind: 'k', title: 't' })
    service.publish({ id: 'a', kind: 'k', title: 't2' })
    service.setRead('a')
    service.setRead('a')
    service.dismiss('a')
    service.delete('a')
    service.delete('a')

    expect(updates).toEqual(['a', 'a', 'a', 'a'])
    expect(removals).toEqual(['a'])
  })

  it('contains sync listener throws so later listeners still run', () => {
    const { ctx, service } = boot()
    const seen: string[] = []
    const warnSpy: unknown[] = []
    ctx.logger.warn = ((...args: unknown[]) => { warnSpy.push(args) }) as typeof ctx.logger.warn
    ctx.on('notifications/updated', () => { throw new Error('boom') })
    ctx.on('notifications/updated', (id) => { seen.push(id) })
    service.publish({ id: 'a', kind: 'k', title: 't' })
    expect(seen).toEqual(['a'])
    expect(warnSpy.length).toBeGreaterThan(0)
  })

  it('contains async listener rejections', async () => {
    const { ctx, service } = boot()
    const seen: string[] = []
    const rejections: unknown[] = []
    ctx.logger.warn = ((error: unknown) => { rejections.push(error) }) as typeof ctx.logger.warn
    ctx.on('notifications/updated', () => Promise.reject(new Error('late boom')))
    ctx.on('notifications/updated', (id) => { seen.push(id) })
    service.publish({ id: 'a', kind: 'k', title: 't' })
    expect(seen).toEqual(['a'])
    await new Promise(resolve => setTimeout(resolve, 0))
    expect(rejections.some(entry => entry instanceof Error)).toBe(true)
  })

  it('rethrows an INVARIANT-coded listener failure after every listener ran', () => {
    const { ctx, service } = boot()
    const seen: string[] = []
    const invariant = Object.assign(new Error('seam violated'), { code: 'INVARIANT' })
    ctx.on('notifications/updated', () => { throw invariant })
    ctx.on('notifications/updated', (id) => { seen.push(id) })
    expect(() => service.publish({ id: 'a', kind: 'k', title: 't' })).toThrow(invariant)
    expect(seen).toEqual(['a'])
    // The mutation itself committed before the fan-out.
    expect(service.list().map(view => view.id)).toEqual(['a'])
  })
})

describe('persistence', () => {
  it('round-trips state across re-instantiation', () => {
    const home = freshHome()
    const first = boot(home)
    first.service.publish({ id: 'keep', kind: 'k', title: 'kept', body: 'b', data: { x: 1 } })
    first.service.setRead('keep')
    first.service.dismiss('keep')

    const secondCtx = new Context()
    const second = new NotificationsService(secondCtx, { dshHome: home })
    expect(second.list()).toEqual(first.service.list())
  })

  it('recovers an empty store from a corrupt file and warns once', () => {
    const home = freshHome()
    mkdirSync(join(home, 'notifications', 'v1'), { recursive: true })
    writeFileSync(join(home, 'notifications', 'v1', 'state.json'), '{not json', 'utf8')
    const { ctx, service } = boot(home)
    const warnings: unknown[] = []
    ctx.logger.warn = ((...args: unknown[]) => { warnings.push(args) }) as typeof ctx.logger.warn

    expect(service.list()).toHaveLength(0)
    service.publish({ id: 'fresh', kind: 'k', title: 't' })
    service.publish({ id: 'fresh2', kind: 'k', title: 't' })
    expect(warnings.filter(args => String((args as string[])[0]).includes('ignoring corrupt store'))).toHaveLength(1)

    const thirdCtx = new Context()
    expect(new NotificationsService(thirdCtx, { dshHome: home }).list().map(view => view.id))
      .toEqual(['fresh2', 'fresh'])
  })

  it('treats structurally invalid documents as corrupt', () => {
    const home = freshHome()
    const file = join(home, 'notifications', 'v1', 'state.json')
    mkdirSync(join(home, 'notifications', 'v1'), { recursive: true })
    writeFileSync(file, JSON.stringify({ version: 99, notifications: [] }), 'utf8')
    expect(boot(home).service.list()).toHaveLength(0)

    writeFileSync(file, JSON.stringify({ version: 1, notifications: [{ id: 'x' }] }), 'utf8')
    expect(boot(home).service.list()).toHaveLength(0)

    writeFileSync(file, JSON.stringify([1, 2]), 'utf8')
    expect(boot(home).service.list()).toHaveLength(0)
  })

  it('surfaces non-absence read errors instead of treating them as corruption', () => {
    const home = freshHome()
    mkdirSync(join(home, 'notifications', 'v1', 'state.json'), { recursive: true })
    expect(() => boot(home).service.list()).toThrow()
  })
})

describe('store format helpers', () => {
  it('parses what renderStore wrote and validates rows strictly', () => {
    const rows = [{
      id: 'a', kind: 'k', title: 't', createdAt: '2026-01-01T00:00:00.000Z', read: false, dismissed: false,
    }]
    expect(parseStore(renderStore(rows))).toEqual(rows)
    for (const broken of [
      '{"version":1,"notifications":{}}',
      '{"version":1,"notifications":[null]}',
      '{"version":1,"notifications":[{"id":"","kind":"k","title":"t","createdAt":"x","read":true,"dismissed":false}]}',
      '{"version":1,"notifications":[{"id":"a","kind":7,"title":"t","createdAt":"x","read":true,"dismissed":false}]}',
      '{"version":1,"notifications":[{"id":"a","kind":"k","title":8,"createdAt":"x","read":true,"dismissed":false}]}',
      '{"version":1,"notifications":[{"id":"a","kind":"k","title":"t","body":true,"createdAt":"x","read":true,"dismissed":false}]}',
      '{"version":1,"notifications":[{"id":"a","kind":"k","title":"t","data":"nope","createdAt":"x","read":true,"dismissed":false}]}',
      '{"version":1,"notifications":[{"id":"a","kind":"k","title":"t","createdAt":5,"read":true,"dismissed":false}]}',
      '{"version":1,"notifications":[{"id":"a","kind":"k","title":"t","createdAt":"x","read":"yes","dismissed":false}]}',
      '{"version":1,"notifications":[{"id":"a","kind":"k","title":"t","createdAt":"x","read":true,"dismissed":"no"}]}',
      '"root"',
    ]) {
      expect(() => parseStore(broken)).toThrow()
    }
  })
})

describe('atomic writes', () => {
  it('leaves no temp residue behind after success or failure', () => {
    const home = freshHome()
    const target = join(home, 'nested', 'state.json')
    writeFileAtomicSync(target, 'one')
    expect(readFileSync(target, 'utf8')).toBe('one')

    mkdirSyncGuard(join(home, 'blocked'))
    expect(() => writeFileAtomicSync(join(home, 'blocked'), 'two')).toThrow()
    expect(readdirSync(home).filter(name => name.endsWith('.tmp'))).toEqual([])
    expect(existsSync(target)).toBe(true)
  })
})

/** Create an empty directory used as a rename target; renaming a file onto it fails. */
function mkdirSyncGuard(path: string): void {
  rmSync(path, { recursive: true, force: true })
  mkdirSync(path, { recursive: true })
}
