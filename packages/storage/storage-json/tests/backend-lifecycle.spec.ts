import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterAll, afterEach, describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import { z } from 'zod'
import Storage from '@deepseek-ai/dsh-storage'
import * as JsonPlugin from '../src/index.ts'
import * as DomainPlugin from '../../storage-domain/src/index.ts'
import { defineDomain, domainTable } from '../../storage-domain/src/index.ts'

const spec = defineDomain({
  name: 'race',
  version: 1,
  tables: { rows: domainTable<string, { label: string }>(z.object({ label: z.string() })) },
})

const roots: string[] = []

async function freshRoot(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), 'dsh-storage-json-lifecycle-'))
  roots.push(root)
  return root
}

afterAll(async () => {
  for (const root of roots) await rm(root, { recursive: true, force: true })
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('backend plugin lifecycle', () => {
  it('keeps the medium writable for a dependent domain write while the backend fiber unloads', async () => {
    const root = await freshRoot()
    const ctx = new Context()
    await ctx.plugin(Storage)
    const backendFiber = await ctx.plugin(JsonPlugin, { root })
    const domainFiber = await ctx.plugin(DomainPlugin, { backend: 'json' })

    const backend = ctx.storage.backend.get('json')
    const kv = backend.kv!
    const openUnit = kv.open.bind(kv)

    // The pause seam: the unit the domain receives holds its first
    // putRecord until the test releases it, so the queued write sits
    // immediately before its last durable step.
    const releaseWrite = Promise.withResolvers<undefined>()
    const writeParked = Promise.withResolvers<undefined>()
    let firstWrite = true
    vi.spyOn(kv, 'open').mockImplementation(async (descriptor) => {
      const unit = await openUnit(descriptor)
      const putRecord = unit.putRecord.bind(unit)
      unit.putRecord = async (table, key, value) => {
        if (firstWrite) {
          firstWrite = false
          writeParked.resolve(undefined)
          await releaseWrite.promise
        }
        return putRecord(table, key, value)
      }
      return unit
    })

    const order: string[] = []
    const closeBackend = backend.close.bind(backend)
    vi.spyOn(backend, 'close').mockImplementation(async () => {
      order.push('medium-close')
      await closeBackend()
    })

    const domain = await ctx.storageDomain.open(spec)
    const write = domain.table('rows').put('a', { label: 'late' })
    const writeSettled = write.then(
      () => { order.push('write-settled') },
      (error: unknown) => {
        order.push('write-rejected')
        throw error
      },
    )
    await writeParked.promise

    // A config-patch reload of the backend plugin: its own fiber unloads.
    const disposing = backendFiber.dispose().then(() => { order.push('disposal-settled') })
    // One turn of the event loop: a sibling disposer would have reached the
    // medium close by now, an ordered one still waits for the withdrawal.
    await new Promise(resolve => setTimeout(resolve, 0))

    // The medium stays open and the disposal stays pending while the write
    // is parked.
    expect(order).toEqual([])

    releaseWrite.resolve(undefined)
    await writeSettled
    await disposing

    // The close runs only after the write settled.
    expect(order).toEqual(['write-settled', 'medium-close', 'disposal-settled'])

    // The record reached the medium.
    const document = JSON.parse(await readFile(join(root, 'race.json'), 'utf8')) as {
      tables: Record<string, Record<string, unknown>>
    }
    expect(document.tables['rows']?.['a']).toEqual({ label: 'late' })

    await domainFiber.dispose()
  })
})
