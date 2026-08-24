/**
 * Composition-reference closure gate: every `name: '@deepseek-ai/…'` plugin
 * reference across composition YAML must resolve to something loadable at
 * boot — a workspace package, an installed (vendored/rescoped) package in the
 * hoisted tree, or a declared subpath export of such a package. Catches
 * dangling registrations at authoring time; before this gate existed, a row
 * plus its matching dependency entry for a nonexistent package passed
 * verify-cordis-config (self-consistency only) and exploded only at host
 * boot, killing the whole plugin tree (web-fetch-moli / browser-moli,
 * 2026-08-23).
 *
 * Part of `bun run hygiene`; runs standalone as
 * `bun run verify-composition-references`.
 * @module scripts/verify-composition-references
 */
import { existsSync, globSync, readdirSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { join, relative } from 'node:path'

const root = fileURLToPath(new URL('..', import.meta.url))

/** Names that are loader vocabulary rather than package references. */
const NON_PACKAGE_NAMES = new Set([
  '@deepseek-ai/cordis',
  '@deepseek-ai/cordis-plugin-group',
  '@deepseek-ai/cordis-plugin-include',
])

function* yamlFiles(dir: string): Generator<string> {
  for (const item of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, item.name)
    if (item.name === 'node_modules') continue
    if (item.isDirectory()) yield* yamlFiles(path)
    else if (/\.ya?ml$/.test(item.name)) yield path
  }
}

const workspacePackages = new Map()
for (const pattern of ['packages/*/*/package.json', 'packages/*/*/*/package.json', 'packages/*/*/*/*/package.json']) {
  for (const manifest of globSync(pattern, { cwd: root })) {
    workspacePackages.set(JSON.parse(readFileSync(join(root, manifest), 'utf8')).name, manifest)
  }
}

/** A name resolves when a workspace package or a hoisted installed package carries it. */
function packageExists(name: string): boolean {
  return workspacePackages.has(name)
    || existsSync(join(root, 'node_modules', name, 'package.json'))
}

/** Subpath `@scope/pkg/sub` resolves when the parent declares a matching export. */
function subpathResolves(name: string): boolean {
  // Split at the slash AFTER the scoped package name — the scope's own slash
  // (`@deepseek-ai/…`) is part of the package identity, not a subpath mark.
  const match = /^(@[^/]+\/[^/]+)\/(.+)$/.exec(name)
  if (match === null) return false
  const parent: string = match[1] ?? ''
  const sub: string = match[2] ?? ''
  const relManifest = workspacePackages.get(parent)
  let manifestPath: string
  if (relManifest !== undefined) manifestPath = join(root, relManifest)
  else if (existsSync(join(root, 'node_modules', parent, 'package.json'))) manifestPath = join(root, 'node_modules', parent, 'package.json')
  else return false
  const exports_ = JSON.parse(readFileSync(manifestPath, 'utf8')).exports
  if (exports_ === undefined || exports_ === null || typeof exports_ !== 'object') return false
  return Object.keys(exports_).some(key => key === `./${sub}` || key === './*' || key === '.')
}

const failures: string[] = []
for (const dir of ['apps', 'packages']) {
  for (const path of yamlFiles(join(root, dir))) {
    const source = readFileSync(path, 'utf8')
    for (const match of source.matchAll(/name:\s*'(@deepseek-ai\/[^']+)'/g)) {
      const name: string = match[1] ?? ''
      if (NON_PACKAGE_NAMES.has(name) || packageExists(name) || subpathResolves(name)) continue
      failures.push(`${relative(root, path)}: references "${name}" which does not exist as a workspace package, installed package, or declared export`)
    }
  }
}

if (failures.length > 0) {
  console.error(`verify-composition-references: ${String(failures.length)} dangling plugin reference(s):`)
  for (const failure of failures) console.error(`  ${failure}`)
  process.exit(1)
}
console.log('verify-composition-references: all plugin references resolve.')
