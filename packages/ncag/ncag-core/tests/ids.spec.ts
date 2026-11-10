import { describe, expect, it } from 'vitest'
import { adoptId, newId } from '../src/ids.ts'
import type { IdKind } from '../src/ids.ts'

const PREFIXES: readonly (readonly [IdKind, string])[] = [
  ['project', 'prj'], ['request', 'req'], ['requirement', 'rq'], ['task', 'tsk'],
  ['agent', 'agt'], ['profile', 'prf'], ['message', 'msg'], ['decision', 'dec'],
  ['finding', 'fnd'], ['lease', 'lse'], ['changeset', 'chg'], ['audit', 'aud'],
  ['checkpoint', 'ckp'], ['correlation', 'cor'],
]

describe('ncag identifier minting', () => {
  it('mints a fresh, correctly prefixed identifier for every family', () => {
    const seen = new Set<string>()
    for (const [kind, prefix] of PREFIXES) {
      const first = newId(kind)
      const second = newId(kind)
      expect(first.startsWith(`${prefix}_`)).toBe(true)
      expect(second.startsWith(`${prefix}_`)).toBe(true)
      expect(first).not.toBe(second)
      seen.add(first)
    }
    // One mint per call: no two families share a value.
    expect(seen.size).toBe(PREFIXES.length)
  })

  it('adopts identifier text only when its prefix matches the family', () => {
    const project = newId('project')
    expect(adoptId('project', project)).toBe(project)
    expect(() => adoptId('task', project)).toThrow(/is not a task identifier/)
  })
})
