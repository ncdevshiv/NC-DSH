/** How a Host form schema becomes the fields a settings page renders. */

import { Context } from '@deepseek-ai/cordis'
import Schema from '@deepseek-ai/schemastery'
import { describe, expect, it } from 'vitest'
import { autoFields, humanizeKey, type AutoField } from '../src/client/auto-fields.ts'
import { SettingsSchemaService } from '../src/client/schema.ts'

const service = new SettingsSchemaService(new Context())

/** Classify a schema the way the page does: rehydrate, then walk. */
function classify(schema: Schema): AutoField[] {
  const node = service.rehydrate(JSON.parse(JSON.stringify(schema.toJSON())))
  return autoFields(node)
}

/** One field addressed by key, from a classified form. */
function fieldOf(fields: readonly AutoField[], key: string): AutoField {
  const found = fields.find(candidate => candidate.path[candidate.path.length - 1] === key)
  if (found === undefined) throw new Error(`no field ${key}`)
  return found
}

describe('humanizeKey', () => {
  it('splits camel case and keeps acronyms whole', () => {
    expect(humanizeKey('maxImagesPerRequest')).toBe('Max images per request')
    expect(humanizeKey('baseURL')).toBe('Base URL')
    expect(humanizeKey('toolCallTimeoutMs')).toBe('Tool call timeout ms')
  })

  it('splits on separators and passes an unbreakable key through', () => {
    expect(humanizeKey('retry-policy')).toBe('Retry policy')
    expect(humanizeKey('')).toBe('')
  })
})

describe('autoFields classification', () => {
  it('classifies scalars by their schema type', () => {
    const fields = classify(Schema.object({
      flag: Schema.boolean().volatile(),
      count: Schema.number().volatile(),
      name: Schema.string().volatile(),
    }))
    expect(fields.map(field => field.kind)).toEqual(['toggle', 'number', 'text'])
  })

  it('reads a closed string union as a select carrying every literal', () => {
    const field = fieldOf(
      classify(Schema.object({ reasoningEffort: Schema.union(['off', 'low', 'high'] as const).volatile() })),
      'reasoningEffort',
    )
    expect(field).toMatchObject({
      kind: 'select',
      options: [{ value: 'off' }, { value: 'low' }, { value: 'high' }],
    })
  })

  it('unwraps a one-branch union and treats a bare literal as a single option', () => {
    const fields = classify(Schema.object({
      single: Schema.union([Schema.string()] as const).volatile(),
      literal: Schema.const('in-history').volatile(),
    }))
    expect(fieldOf(fields, 'single').kind).toBe('text')
    expect(fieldOf(fields, 'literal')).toMatchObject({ kind: 'select', options: [{ value: 'in-history' }] })
  })

  it('edits a union of non-literals as JSON rather than guessing a control', () => {
    const field = fieldOf(
      classify(Schema.object({
        budget: Schema.union([Schema.number(), Schema.const('low')] as const).volatile(),
      })),
      'budget',
    )
    expect(field.kind).toBe('json')
  })

  it('reads an object as a group whose children carry nested paths', () => {
    const field = fieldOf(
      classify(Schema.object({
        retryPolicy: Schema.object({
          attempts: Schema.number().volatile(),
        }).volatile(),
      })),
      'retryPolicy',
    )
    expect(field.kind).toBe('group')
    const group = field as Extract<AutoField, { kind: 'group' }>
    expect(group.children).toHaveLength(1)
    expect(group.children[0]!.path).toEqual(['retryPolicy', 'attempts'])
    expect(group.children[0]!.kind).toBe('number')
  })

  it('edits arrays and dictionaries of scalars as JSON', () => {
    const fields = classify(Schema.object({
      models: Schema.array(Schema.string()).volatile(),
      env: Schema.dict(Schema.string()).volatile(),
    }))
    expect(fieldOf(fields, 'models').kind).toBe('rows')
    expect(fieldOf(fields, 'env').kind).toBe('pairs')
  })

  it('edits an array of objects as JSON', () => {
    const field = fieldOf(
      classify(Schema.object({
        models: Schema.array(Schema.object({ id: Schema.string() })).volatile(),
      })),
      'models',
    )
    expect(field.kind).toBe('json')
  })

  it('omits a field the schema hides', () => {
    const fields = classify(Schema.object({
      shown: Schema.string().volatile(),
      hidden: Schema.string().volatile().hidden(true),
    }))
    expect(fields.map(field => field.path[0])).toEqual(['shown'])
  })

  it('renders a non-object form as one field rather than nothing', () => {
    const fields = classify(Schema.string())
    expect(fields).toHaveLength(1)
    expect(fields[0]!.kind).toBe('text')
  })

  it('carries the required flag from the schema', () => {
    const fields = classify(Schema.object({
      needed: Schema.string().required().volatile(),
      optional: Schema.string().volatile(),
    }))
    expect(fieldOf(fields, 'needed').required).toBe(true)
    expect(fieldOf(fields, 'optional').required).toBe(false)
  })

  it('prefers a declared description over the humanized key', () => {
    const field = fieldOf(
      classify(Schema.object({
        maxTokens: Schema.string().volatile().description('Cap on generated tokens'),
      })),
      'maxTokens',
    )
    expect(field.label).toBe('Cap on generated tokens')
    expect(field.hint).toBe('Cap on generated tokens')
  })

  it('merges a discriminated union of objects into one field list', () => {
    // The shape a closed variant Config takes, as the MCP transport choice
    // and the browser launch/attach choice are both written.
    const fields = classify(Schema.union([
      Schema.object({
        mode: Schema.const('launch'),
        headless: Schema.boolean(),
        toolCallTimeoutMs: Schema.number(),
      }),
      Schema.object({
        mode: Schema.const('attach'),
        endpoint: Schema.string(),
        toolCallTimeoutMs: Schema.number(),
      }),
    ]))
    expect(fields).toHaveLength(1)
    const group = fields[0] as Extract<AutoField, { kind: 'group' }>
    // A key only some variants declare is kept: it is editable for those.
    const keys = group.children.map(child => child.path[child.path.length - 1]).sort()
    expect(keys).toEqual(['endpoint', 'headless', 'mode', 'toolCallTimeoutMs'])
    // A key every variant shares is described once.
    expect(group.children.filter(child => child.path[child.path.length - 1] === 'toolCallTimeoutMs')).toHaveLength(1)
  })

  it('classifies every field the shipped llm-deepseek form declares', () => {
    // The shape of the volatile subtree this product actually serves today:
    // scalars, a closed union, an array of objects, and a nested object.
    const fields = classify(Schema.object({
      baseURL: Schema.string().volatile(),
      thinking: Schema.union(['enabled', 'disabled'] as const).volatile(),
      reasoningEffort: Schema.union(['off', 'low', 'high', 'max'] as const).volatile(),
      maxTokens: Schema.number().default(8192).volatile(),
      models: Schema.array(Schema.object({ id: Schema.string(), contextWindow: Schema.number() })).volatile(),
      retryPolicy: Schema.object({ attempts: Schema.number() }).volatile(),
      fileExpiresAfterSeconds: Schema.number().default(86_400).volatile(),
    }))
    // Every declared field yields exactly one descriptor: none is dropped.
    expect(fields).toHaveLength(7)
    expect(fields.filter(field => field.kind === 'group')).toHaveLength(1)
    expect(fields.filter(field => field.kind === 'select')).toHaveLength(2)
  })
})
