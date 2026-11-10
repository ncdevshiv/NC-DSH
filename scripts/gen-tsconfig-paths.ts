/**
 * Expand the workspace path aliases that a wildcard would otherwise resolve by
 * probing every package group in turn.
 *
 * `tsconfig.base.json` is the resolution facade for the whole repository. A
 * `@deepseek-ai/dsh-*` wildcard would list one candidate glob per package
 * group. TypeScript and tsx try those candidates in order, so a specifier
 * whose package sits late in the list pays for every earlier miss. Under tsx's
 * ESM hook each miss is an `ERR_MODULE_NOT_FOUND` that Node decorates with a
 * full CommonJS resolution walk, which dominated source-launch boot.
 *
 * This generator writes one explicit entry per package into a marked region of
 * `tsconfig.base.json`, leaving every hand-written alias and comment outside
 * that region untouched. `--check` reports drift instead of writing, so a new
 * package that needs an alias fails a gate rather than silently resolving
 * through a fallback that no longer exists.
 *
 * @module scripts/gen-tsconfig-paths
 */

import { readFileSync, writeFileSync, existsSync, readdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = fileURLToPath(new URL('..', import.meta.url))
const CONFIG = join(ROOT, 'tsconfig.base.json')
const BEGIN = '      // BEGIN generated package aliases — bun run gen-tsconfig-paths'
const END = '      // END generated package aliases'

/** Package-name prefix the expanded aliases cover. */
const PREFIX = '@deepseek-ai/dsh-'

/** One workspace package the generated region maps. */
interface PackageAlias {
  /** Bare specifier, e.g. `@deepseek-ai/dsh-session`. */
  readonly specifier: string
  /** Repository-relative source directory, e.g. `./packages/session/session/src`. */
  readonly source: string
}

/**
 * Two package directories that declare the same npm name.
 */
export interface ShadowedPackage {
  /** The contested npm name. */
  readonly name: string
  /** The group-order-first directory — the one the alias maps. */
  readonly winner: string
  /** The later directory, which the tiebreak shadowed. */
  readonly loser: string
}

/**
 * Read a workspace manifest's declared name.
 * @param manifest - absolute path to a `package.json`.
 * @returns The declared name, or undefined when the file is absent or nameless.
 */
function packageName(manifest: string): string | undefined {
  let parsed: unknown
  try {
    parsed = JSON.parse(readFileSync(manifest, 'utf8'))
  } catch (_absentOrUnreadableManifest) {
    return undefined
  }
  if (typeof parsed !== 'object' || parsed === null) return undefined
  const name: unknown = (parsed as { name?: unknown }).name
  return typeof name === 'string' ? name : undefined
}

/** One workspace package directory and the name its manifest declares. */
interface WorkspacePackage {
  readonly group: string
  readonly directory: string
  readonly name: string
}

/**
 * Walk `packages/<group>/<directory>` once, in a stable order.
 * @returns Every directory whose manifest names a `@deepseek-ai/dsh-` package and that carries `src`.
 */
function workspacePackages(): WorkspacePackage[] {
  const packages = join(ROOT, 'packages')
  const found: WorkspacePackage[] = []
  for (const group of readdirSync(packages).sort()) {
    const groupDir = join(packages, group)
    if (!statSync(groupDir).isDirectory()) continue
    for (const directory of readdirSync(groupDir).sort()) {
      const packageDir = join(groupDir, directory)
      const name = packageName(join(packageDir, 'package.json'))
      if (name === undefined || !name.startsWith(PREFIX)) continue
      if (existsSync(join(packageDir, 'src'))) found.push({ group, directory, name })
    }
  }
  return found
}

/**
 * Collect every package the removed wildcards could resolve.
 *
 * A wildcard substituted the specifier's suffix into `packages/<group>/<suffix>/src`,
 * so it only ever resolved a package whose declared name is exactly
 * `@deepseek-ai/dsh-<directory>`. Packages named after something other than
 * their directory already carry a hand-written alias and are skipped here.
 *
 * Two directories can declare the SAME name — `packages/experimental/hook-protocol`
 * and `packages/hooks/hook-protocol` both declare
 * `@deepseek-ai/dsh-hook-protocol`, and five more pairs do the same. The
 * removed wildcard resolved that by GROUP ORDER, so the explicit map has to
 * express the same tiebreak rather than refuse to: the group-order-first
 * directory wins, exactly as the wildcard resolved it, and the shadowed
 * duplicate is REPORTED so the ambiguity stays visible instead of becoming
 * a silent resolution.
 *
 * @returns Aliases sorted by specifier, and the duplicates the tiebreak shadowed.
 */
export function collectPackageAliases(): { aliases: PackageAlias[]; shadowed: ShadowedPackage[] } {
  const bySpecifier = new Map<string, PackageAlias & { directory: string }>()
  const shadowed: ShadowedPackage[] = []
  for (const { group, directory, name } of workspacePackages()) {
    if (name !== `${PREFIX}${directory}`) continue
    const previous = bySpecifier.get(name)
    if (previous !== undefined) {
      // The walk is group-major over sorted groups, so `previous` is the
      // group-order-first claimant — the one the removed wildcard resolved
      // to. The later duplicate loses, and is reported rather than
      // silently dropped.
      shadowed.push({
        name,
        winner: previous.directory,
        loser: `${group}/${directory}`,
      })
      continue
    }
    bySpecifier.set(name, {
      specifier: name,
      source: `./packages/${group}/${directory}/src`,
      directory: `${group}/${directory}`,
    })
  }
  return {
    aliases: [...bySpecifier.values()]
      .map(({ specifier, source }) => ({ specifier, source }))
      .sort((left, right) => left.specifier.localeCompare(right.specifier)),
    shadowed,
  }
}

/**
 * Collect every workspace package the aliases must cover.
 *
 * Unlike {@link collectPackageAliases} this keeps packages whose name does not
 * match their directory. The generator cannot map those — only a hand-written
 * alias can — but they still have to be mapped by something, because deleting
 * the group wildcards removed the fallback that used to catch them.
 *
 * @returns Declared names of every `@deepseek-ai/dsh-` package carrying a `src` directory.
 */
export function collectPackageNames(): string[] {
  return workspacePackages()
    .map(({ name }) => name)
    .sort((left, right) => left.localeCompare(right))
}

/**
 * Read the bare package specifiers a config maps, generated region included.
 * @param text - `tsconfig.base.json` contents.
 * @returns Specifiers mapped without a subpath.
 */
export function mappedSpecifiers(text: string): Set<string> {
  const keys = new Set<string>()
  for (const match of text.matchAll(/^\s*"(@deepseek-ai\/dsh-[^"/]+)":/gm)) {
    const key = match[1]
    if (key !== undefined) keys.add(key)
  }
  return keys
}

/**
 * Report packages that no alias maps.
 *
 * A package missing from `paths` still resolves — through the workspace symlink
 * and the package's own `exports` — but to built `lib/` output rather than to
 * source, which is the artifact-plane leak the explicit aliases exist to avoid.
 * Naming it here turns that into a gate failure instead of a silent difference.
 *
 * @param packages - every workspace package that needs an alias.
 * @param mapped - bare specifiers the config maps.
 * @returns Unmapped package names, in the order given.
 */
export function uncoveredPackages(
  packages: readonly string[],
  mapped: ReadonlySet<string>,
): string[] {
  return packages.filter(name => !mapped.has(name))
}

/**
 * Render the generated region's alias lines.
 * @param aliases - packages to map, in emission order.
 * @param handWritten - specifiers already mapped outside the region; a duplicate key would shadow one silently.
 * @returns The region body, one JSON member per line.
 */
export function renderAliases(aliases: readonly PackageAlias[], handWritten: ReadonlySet<string>): string {
  const lines: string[] = []
  for (const alias of aliases) {
    if (!handWritten.has(alias.specifier)) {
      lines.push(`      ${JSON.stringify(alias.specifier)}: [${JSON.stringify(alias.source)}]`)
    }
  }
  // The region closes `paths`, so the last member carries no trailing comma.
  return lines.join(',\n')
}

/**
 * Replace the generated region of a config's text.
 * @param text - current `tsconfig.base.json` contents.
 * @param body - rendered alias lines.
 * @returns The updated contents.
 * @throws When the markers are missing or out of order.
 */
export function writeRegion(text: string, body: string): string {
  const begin = text.indexOf(BEGIN)
  const end = text.indexOf(END)
  if (begin < 0 || end < begin) {
    throw new Error(`gen-tsconfig-paths: ${CONFIG} is missing the generated-region markers.`)
  }
  return `${text.slice(0, begin)}${BEGIN}\n${body}\n${END}${text.slice(end + END.length)}`
}

/**
 * Parse the config's `paths` keys, ignoring the generated region.
 * @param text - current `tsconfig.base.json` contents.
 * @returns Specifiers mapped by hand.
 */
function handWrittenSpecifiers(text: string): Set<string> {
  const begin = text.indexOf(BEGIN)
  const end = text.indexOf(END)
  const outside = begin < 0 || end < begin ? text : text.slice(0, begin) + text.slice(end)
  const keys = new Set<string>()
  for (const match of outside.matchAll(/^\s*"(@deepseek-ai\/[^"]+)":/gm)) {
    const key = match[1]
    if (key !== undefined) keys.add(key)
  }
  return keys
}

if (process.argv[1] && import.meta.filename === resolve(process.argv[1])) {
  const check = process.argv.includes('--check')
  const current = readFileSync(CONFIG, 'utf8')
  const { aliases, shadowed } = collectPackageAliases()
  const next = writeRegion(current, renderAliases(aliases, handWrittenSpecifiers(current)))
  const uncovered = uncoveredPackages(collectPackageNames(), mappedSpecifiers(next))
  // The shadowed duplicates are the packages two directories declare the
  // same npm name for. The tiebreak resolves them the way the removed
  // wildcard did, so this is a report, not a failure: the build must not
  // stop over an ambiguity the alias map already encodes.
  for (const { name, winner, loser } of shadowed) {
    console.warn(
      `gen-tsconfig-paths: ${name} is declared by packages/${winner} and packages/${loser}; `
      + `the alias maps the group-order-first one (packages/${winner}).`,
    )
  }
  if (uncovered.length > 0) {
    console.error(
      'gen-tsconfig-paths: no alias maps '
      + `${uncovered.join(', ')}; add a hand-written entry, because a package named after `
      + 'something other than its directory cannot be generated.',
    )
    process.exitCode = 1
  } else if (current === next) {
    console.log('gen-tsconfig-paths: tsconfig.base.json package aliases are current.')
  } else if (check) {
    console.error('gen-tsconfig-paths: tsconfig.base.json is stale; run `bun run gen-tsconfig-paths`.')
    process.exitCode = 1
  } else {
    writeFileSync(CONFIG, next)
    console.log('gen-tsconfig-paths: rewrote tsconfig.base.json package aliases.')
  }
}
