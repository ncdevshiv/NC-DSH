/**
 * That the terminal's typography is served and editable.
 *
 * The xterm instance opened with a hardcoded face and size before this
 * preference existed, so these assert against the shipped schema that both are
 * now real, persisted, and range-checked — and that the defaults reproduce the
 * face the terminal opened with, so an existing deployment renders identically
 * until the user picks.
 */

import Schema from '@deepseek-ai/schemastery'
import { describe, expect, it } from 'vitest'
import { isVolatilePath, projectForm, volatileForm } from '../../../settings/settings/src/schema.ts'

import {
  Config, DEFAULT_TERMINAL_FONT_FAMILY, DEFAULT_TERMINAL_FONT_SIZE,
  TERMINAL_FONT_FAMILIES, TERMINAL_FONT_SIZE_MAX, TERMINAL_FONT_SIZE_MIN,
  TERMINAL_SETTINGS_NAMESPACE,
} from '../src/terminal-settings.ts'

describe('terminal typography settings', () => {
  it('serves a form, because the namespace is what the rows observe', () => {
    expect(volatileForm(Config)).toBeDefined()
  })

  it('exposes exactly the two fields a user changes', () => {
    expect(volatileForm(Config)?.type).toBe('object')
    for (const field of ['fontFamily', 'fontSize']) {
      expect(isVolatilePath(Config, [field])).toBe(true)
    }
    const dict = volatileForm(Config)?.dict ?? {}
    // Exactly the two fields, so the rows can never render a third.
    expect(Object.keys(dict).sort()).toEqual(['fontFamily', 'fontSize'])
  })

  it('defaults to the face and size the xterm instance opened with', () => {
    // The hardcoded value before this preference existed.
    expect(DEFAULT_TERMINAL_FONT_FAMILY).toBe('ui-monospace, SFMono-Regular, Menlo, Consolas, monospace')
    expect(DEFAULT_TERMINAL_FONT_SIZE).toBe(13)
    // Each editable field is its own reference, so a section reads them apart.
    const resolved = Config({})
    expect(resolved.fontFamily.get()).toBe(DEFAULT_TERMINAL_FONT_FAMILY)
    expect(resolved.fontSize.get()).toBe(DEFAULT_TERMINAL_FONT_SIZE)
  })

  it('accepts a chosen stack and a size inside the range', () => {
    for (const family of TERMINAL_FONT_FAMILIES) {
      expect(Config({ fontFamily: family }).fontFamily.get()).toBe(family)
    }
    expect(Config({ fontSize: TERMINAL_FONT_SIZE_MIN }).fontSize.get()).toBe(TERMINAL_FONT_SIZE_MIN)
    expect(Config({ fontSize: TERMINAL_FONT_SIZE_MAX }).fontSize.get()).toBe(TERMINAL_FONT_SIZE_MAX)
  })
  it('rejects a stack that is not offered and a size outside the range', () => {
    expect(() => Config({ fontFamily: 'Comic Sans MS' as never })).toThrow()
    expect(() => Config({ fontSize: TERMINAL_FONT_SIZE_MIN - 1 })).toThrow()
    expect(() => Config({ fontSize: TERMINAL_FONT_SIZE_MAX + 1 })).toThrow()
    expect(() => Config({ fontSize: 12.5 })).toThrow()
  })

  it('projects the persisted section the rows read', () => {
    const resolved = Config({ fontFamily: 'Consolas, monospace', fontSize: 16 })
    expect(projectForm(volatileForm(Config) as Schema, {
      fontFamily: resolved.fontFamily.get(),
      fontSize: resolved.fontSize.get(),
    })).toEqual({
      fontFamily: 'Consolas, monospace',
      fontSize: 16,
    })
  })

  it('round-trips through the envelope the Host sends', () => {
    const wire = new Schema(JSON.parse(JSON.stringify(Config.toJSON())) as Schema)
    expect(TERMINAL_SETTINGS_NAMESPACE).toBe('ui-sidebar-terminal')
    expect(wire.type).toBe('object')
    expect(Object.keys(wire.dict ?? {}).sort()).toEqual(['fontFamily', 'fontSize'])
  })
})
