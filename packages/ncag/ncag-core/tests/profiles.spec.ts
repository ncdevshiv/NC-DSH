import { describe, expect, it } from 'vitest'
import { BUILT_IN_PROFILES, builtInProfile, profilesForCapability, tierForCapability, validateProfileSeed } from '../src/profiles.ts'
import type { ProfileSeed } from '../src/profiles.ts'

function seed(overrides: Partial<ProfileSeed> = {}): ProfileSeed {
  const base = BUILT_IN_PROFILES.find(profile => profile.key === 'general-purpose')
  if (base === undefined) throw new Error('the general-purpose profile must ship')
  return { ...base, ...overrides }
}

describe('ncag agent profiles', () => {
  it('ships the sixteen built-in specializations', () => {
    expect(BUILT_IN_PROFILES).toHaveLength(16)
    expect(new Set(BUILT_IN_PROFILES.map(profile => profile.key)).size).toBe(16)
  })

  it('materializes a built-in seed with a stable registry identity', () => {
    const profile = builtInProfile(seed())
    expect(profile.builtIn).toBe(true)
    expect(profile.id).toBe('prf_builtin_general-purpose')
  })

  it('accepts a valid extension profile', () => {
    const candidate = seed({ key: 'ml-engineer' })
    expect(validateProfileSeed(candidate, [])).toBe(candidate)
  })

  it('refuses a seed whose key is not a valid registry key', () => {
    expect(() => validateProfileSeed(seed({ key: 'A' }), [])).toThrow(/must be 3-40 lowercase/)
  })

  it('refuses a key that is already registered', () => {
    expect(() => validateProfileSeed(seed({ key: 'ml-engineer' }), ['ml-engineer'])).toThrow(/already registered/)
  })

  it('refuses a seed that states no capability, identity, responsibility or restriction', () => {
    expect(() => validateProfileSeed(seed({ capabilities: [] }), [])).toThrow(/at least one capability/)
    expect(() => validateProfileSeed(seed({ identity: '   ' }), [])).toThrow(/state its identity/)
    expect(() => validateProfileSeed(seed({ responsibilities: [] }), [])).toThrow(/state its responsibilities/)
    expect(() => validateProfileSeed(seed({ restrictions: [] }), [])).toThrow(/state its restrictions/)
  })

  it('selects the profiles that provide one capability', () => {
    const profiles = BUILT_IN_PROFILES.map(builtInProfile)
    const reviewers = profilesForCapability(profiles, 'code-review')
    expect(reviewers.map(profile => profile.key)).toEqual(['code-reviewer'])
    expect(profilesForCapability(profiles, 'no-such-capability')).toEqual([])
  })

  it('reports the tier a capability prefers, falling back to standard', () => {
    const profiles = BUILT_IN_PROFILES.map(builtInProfile)
    expect(tierForCapability(profiles, 'architecture')).toBe('reasoning')
    expect(tierForCapability(profiles, 'no-such-capability')).toBe('standard')
  })
})
