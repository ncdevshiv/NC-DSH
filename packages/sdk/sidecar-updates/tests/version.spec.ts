import { describe, expect, it } from 'vitest'
import { compareVersions } from '../src/version.ts'

describe('compareVersions', () => {
  it.each([
    ['v1.2.3', 'v1.2.4', -1],
    ['v1.2.4', 'v1.2.3', 1],
    ['v1.2.0', 'v1.2', 0],
    ['v1.2', 'v1.2.0', 0],
    ['1.2.9', '1.2.10', -1],
    ['v2.0.0', 'v10.0.0', -1],
    ['v1', 'v1.0.1', -1],
    ['v1.x', 'v1.2', -1],
    ['V1.2.3', 'v1.2.3', 0],
    ['v0.9.9', 'v1.0.0', -1],
    ['v1.2.3', 'v1.2.3', 0],
  ] as const)('orders %s vs %s', (a, b, expectedSign) => {
    const result = compareVersions(a, b)
    expect(Math.sign(result)).toBe(expectedSign)
  })
})
