/**
 * Verify that bun.lock resolves every vendored package name to its workspace
 * source — never a registry copy. The root manifest's workspaces globs plus
 * its overrides entries for the two vendored libraries mapped to
 * `workspace:*` make matching upstream semver ranges resolve to the pinned
 * vendored sources; a registry copy of the same name coexisting with the
 * vendored one silently forks the framework layer (vendor/README.md).
 */
import { readdir, readFile } from 'node:fs/promises'
import { readFileSync } from 'node:fs'
import { join, resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')

async function vendoredNames(): Promise<Set<string>> {
  const names = new Set<string>()
  for (const entry of await readdir(join(root, 'vendor'), { withFileTypes: true })) {
    if (!entry.isDirectory()) continue
    let manifest: { name?: string }
    try {
      manifest = JSON.parse(await readFile(join(root, 'vendor', entry.name, 'package.json'), 'utf8')) as { name?: string }
    } catch {
      continue // not a package directory (e.g. vendor/README.md siblings)
    }
    if (manifest.name !== undefined) names.add(manifest.name)
  }
  return names
}

interface Lockfile {
  /** Workspace path → dependency section → name → declared spec. */
  workspaces?: Record<string, Record<string, Record<string, string>>>
  /** Resolved package entries; registry copies are keyed `<name>@<version>`. */
  packages?: Record<string, unknown>
}

/**
 * Parse the Bun text lockfile: JSON with line comments and trailing commas,
 * neither of which strict `JSON.parse` accepts.
 */
function parseBunLock(text: string): unknown {
  const withoutComments = text.split('\n').filter(line => !line.trimStart().startsWith('//')).join('\n')
  return JSON.parse(withoutComments.replace(/,(\s*[}\]])/g, '$1'))
}

const names = await vendoredNames()
if (names.size === 0) throw new Error('verify-vendored-links: no vendored package manifests found under vendor/')
const lockfile = parseBunLock(readFileSync(join(root, 'bun.lock'), 'utf8')) as Lockfile

const violations: string[] = []

// Workspace resolutions: every dependency entry naming a vendored package must
// declare a workspace spec, or the install silently uses a registry copy.
for (const [importer, sections] of Object.entries(lockfile.workspaces ?? {})) {
  for (const [section, dependencies] of Object.entries(sections)) {
    for (const [dependency, spec] of Object.entries(dependencies)) {
      if (!names.has(dependency)) continue
      if (!spec.startsWith('workspace:')) {
        violations.push(`${importer} ${section}.${dependency} resolves to ${JSON.stringify(spec)} (expected workspace:)`)
      }
    }
  }
}

// Resolved entries: a registry copy materializes as a `<name>@<version>` key,
// at the tree root or nested under its dependent; vendored names must never
// appear there at all.
const packageKeys = Object.keys(lockfile.packages ?? {})
for (const name of names) {
  for (const key of packageKeys) {
    if (key.startsWith(`${name}@`) || key.includes(`/${name}@`)) {
      violations.push(`packages entry ${key} is a registry copy of a vendored package`)
    }
  }
}

if (violations.length > 0) {
  console.error(`verify-vendored-links: ${String(violations.length)} lockfile resolution(s) bypass the vendored workspaces:`)
  for (const violation of violations) console.error(`  - ${violation}`)
  process.exit(1)
}
console.log(`verify-vendored-links: all ${String(names.size)} vendored package names resolve to workspace sources.`)
