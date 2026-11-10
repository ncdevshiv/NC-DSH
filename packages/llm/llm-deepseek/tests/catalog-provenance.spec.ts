import { describe, expect, it } from 'vitest'
import { plainConfig, projectForm, volatileForm } from '../../../settings/settings/src/schema.ts'
import { redactSecrets } from '../../../settings/settings/src/redact.ts'
import { Config, plainOptions } from '../src/config.ts'

type Materialized = { models?: (Record<string, unknown> & { source?: 'discovered' | 'custom' })[] | undefined }

describe('model entry provenance', () => {
  it('declares the marker the Models page stores on a catalog entry', () => {
    // The settings service walks a stored section against this schema, so the
    // marker belongs to the section contract rather than to unknown keys.
    expect(JSON.stringify(Config.toJSON())).toContain('"source"')
    const parsed = plainOptions(Config({ models: [{ id: 'm', source: 'custom' }] }))
    expect((parsed.models?.[0] as unknown as Record<string, unknown>)['source']).toBe('custom')
  })

  it('keeps the marker through the store and describe projections', () => {
    const form = volatileForm(Config)
    if (form === undefined) throw new Error('llm-deepseek has volatile fields')
    const stored = Config({ models: [{ id: 'm', source: 'discovered' }] })
    const value = projectForm(form, plainConfig(stored))
    const redacted = redactSecrets(form as never, value).value
    for (const view of [value, redacted]) {
      expect((view as Materialized).models?.[0]?.['source']).toBe('discovered')
    }
  })
})
