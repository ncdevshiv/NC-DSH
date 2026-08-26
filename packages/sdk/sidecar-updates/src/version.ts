/**
 * Numeric-dot version ordering for release tags. The comparison is
 * deliberately weaker than full semver: release tags here are `v`-prefixed
 * numeric-dot strings, and a missing or non-numeric segment counts as zero so
 * `v1.2` equals `v1.2.0` and orders below `v1.2.1`.
 * @module @deepseek-ai/dsh-sidecar-updates/version
 */

/** Split a tag into its numeric segments, treating absent or non-numeric parts as zero. */
function segmentsOf(tag: string): number[] {
  const bare = tag.startsWith('v') || tag.startsWith('V') ? tag.slice(1) : tag
  return bare.split('.').map((part) => {
    const value = Number(part)
    return Number.isFinite(value) && part.trim().length > 0 ? value : 0
  })
}

/**
 * Compare two version tags numerically per dot-separated segment.
 * @param a - one version tag, with an optional leading `v`.
 * @param b - the other version tag, with an optional leading `v`.
 * @returns negative when `a` orders before `b`, positive after, zero when equal.
 */
export function compareVersions(a: string, b: string): number {
  const left = segmentsOf(a)
  const right = segmentsOf(b)
  const length = Math.max(left.length, right.length)
  for (let index = 0; index < length; index += 1) {
    const delta = (left[index] ?? 0) - (right[index] ?? 0)
    if (delta !== 0) return delta
  }
  return 0
}
