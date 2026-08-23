// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { getPluginMeta, isEssentialPlugin } from '../src/client/plugin-catalog.ts'

describe('plugin catalog', () => {
  it('returns catalog entry for known module', () => {
    const meta = getPluginMeta('@deepseek-ai/dsh-client-ui-renderer')
    expect(meta.summary).toContain('Browser UI renderer')
    expect(meta.impact).toContain('web interface')
  })

  it('falls back for unknown module', () => {
    const meta = getPluginMeta('some-unknown-plugin')
    expect(meta.summary).toBe('Plugin some unknown plugin')
  })

  it('handles cordis builtin', () => {
    const meta = getPluginMeta('cordis:some-builtin')
    expect(meta.summary).toContain('Cordis builtin')
  })

  it('detects essential plugins', () => {
    expect(isEssentialPlugin('@deepseek-ai/dsh-client-ui-renderer')).toBe(true)
    expect(isEssentialPlugin('@deepseek-ai/dsh-client-modules')).toBe(true)
    expect(isEssentialPlugin('@deepseek-ai/dsh-some-other')).toBe(false)
  })
})
