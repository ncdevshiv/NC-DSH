/**
 * Rescope the vendored Cordis packages into the `@deepseek-ai` scope, and undo
 * that rescope with `--reverse`. Every harness package declares `cordis` as a
 * peer dependency, so publication carries this framework layer too; publishing
 * it under the upstream names would squat them on the registry
 * ([rationale](../.agents/notes/implemented/process/2026-08-10-vendor-package-rescope.md),
 * [name mapping](../docs/rescope.md)).
 *
 * The generic pass rewrites ONLY delimited, complete package-name tokens:
 * `'old'` / `"old"` / `` `old` `` / `'old/subpath'`, plus a YAML `name: old`
 * scalar. A match needs a quote (or `name: `) immediately left and the matching
 * quote — optionally after a `/subpath` — immediately right, which excludes
 * `cordis.yml`, the Loader's `cordis:` builtin prefix, `cordis-config-entry`,
 * `@deepseek-ai/dsh-tool-cordis`, and `cordiverse/cordis`, and makes the
 * rewrite idempotent because the scoped name's `cordis` is preceded by `/`.
 * Markdown follows the rename inside every fence, and in `docs/` prose too:
 * a tutorial that teaches an unresolvable name is wrong, while prose elsewhere
 * records what was true when it was written.
 *
 * Sites the token rule cannot express (dot-notation access, unquoted object
 * keys, regex literals, the vendored-manifest table) are listed in
 * {@link EXACT_EDITS} with an exact hit count, so an upstream change to one of
 * them fails loudly instead of being silently skipped.
 *
 * Usage: `bun run rescope-vendor [--apply|--check] [--reverse]`. Without a
 * mode it reports what would change. `--check` asserts the post-state: no
 * residue, every exact edit landed, every postcondition holds, and a second
 * `--apply` would be a no-op.
 */

import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const root = resolve(import.meta.dirname, '..')

/** One vendored package's directory, upstream npm name, and rescoped name. */
interface Rename {
  readonly directory: string
  readonly upstream: string
  readonly scoped: string
}

/** The mapping this codemod applies; `vendor/README.md` carries the same table. */
const RENAMES: readonly Rename[] = [
  { directory: 'cordis', upstream: 'cordis', scoped: '@deepseek-ai/cordis' },
  { directory: 'cosmokit', upstream: 'cosmokit', scoped: '@deepseek-ai/cosmokit' },
  { directory: 'schemastery', upstream: 'schemastery', scoped: '@deepseek-ai/schemastery' },
  { directory: 'loader', upstream: '@cordisjs/plugin-loader', scoped: '@deepseek-ai/cordis-plugin-loader' },
  { directory: 'include', upstream: '@cordisjs/plugin-include', scoped: '@deepseek-ai/cordis-plugin-include' },
  { directory: 'group', upstream: '@cordisjs/plugin-group', scoped: '@deepseek-ai/cordis-plugin-group' },
  { directory: 'timer', upstream: '@cordisjs/plugin-timer', scoped: '@deepseek-ai/cordis-plugin-timer' },
  { directory: 'hmr', upstream: '@cordisjs/plugin-hmr', scoped: '@deepseek-ai/cordis-plugin-hmr' },
  { directory: 'logger-console', upstream: '@cordisjs/plugin-logger-console', scoped: '@deepseek-ai/cordis-plugin-logger-console' },
]

const EXTENSIONS = ['.ts', '.tsx', '.js', '.mjs', '.cjs', '.tpl', '.json', '.yml', '.yaml', '.md'] as const

/** An exact-string edit the token rule cannot express, with its required hit count. */
interface ExactEdit {
  readonly id: string
  readonly file: string
  readonly find: string
  readonly replace: string
  readonly expect: number
}

/**
 * A file where an upstream name also appears as a vendor DIRECTORY name or an
 * upstream runtime identifier: the generic pass is disabled for the listed
 * names and {@link EXACT_EDITS} renames the real package-name occurrences.
 */
interface GenericSkip {
  readonly file: string
  readonly upstream: readonly string[]
}

const GENERIC_SKIPS: readonly GenericSkip[] = [
  // `vendorPackages` lists vendor/ directory names, joined with 'vendor' below it.
  { file: 'packages/examples/acp-demo/tests/built-bin.e2e.ts', upstream: ['cordis', 'cosmokit', 'schemastery'] },
  // `Symbol.for('schemastery')` and the `vendor:` metadata field are upstream identifiers.
  { file: 'vendor/schemastery/src/index.ts', upstream: ['schemastery'] },
  // Asserts the vendored-manifest table, which gains an upstream-name column.
  { file: 'scripts/gen-third-party-notices.spec.ts', upstream: RENAMES.map(rename => rename.upstream) },
  // `cordis` is also an agent-preset id — the directory name under
  // apps/cli/config/agent-presets/ — so in these files the bare name is
  // product data, not a package reference. Renaming it changed which preset
  // the creator flow stages and which id the roster reports.
  { file: 'packages/client/ui-agent-preset/src/client/AgentPresetSection.tsx', upstream: ['cordis'] },
  { file: 'packages/client/ui-agent-preset/src/client/index.ts', upstream: ['cordis'] },
  { file: 'packages/client/ui-agent-preset/tests/apply.client.spec.ts', upstream: ['cordis'] },
  { file: 'packages/client/ui-agent-preset/tests/locales.client.spec.ts', upstream: ['cordis'] },
  { file: 'packages/client/ui-agent-preset/tests/section.client.spec.tsx', upstream: ['cordis'] },
  { file: 'apps/cli/tests/web-agent-presets.e2e.ts', upstream: ['cordis'] },
  { file: 'apps/web/tests/agent-preset-authoring.e2e.ts', upstream: ['cordis'] },
  { file: 'packages/preset/agent-presets/tests/session.spec.ts', upstream: ['cordis'] },
  // The preset's own composition: its header comment and its system prompt name
  // the preset a model mounts, so the scoped name would send the model after an
  // id no roster reports.
  { file: 'apps/cli/config/agent-presets/cordis/agent.cordis.yml', upstream: ['cordis'] },
  // The preset-roster loop names the `cordis` preset id, not a package.
  { file: 'apps/cli/tests/windows-shell.spec.ts', upstream: ['cordis'] },
  // GROUP_ORDER holds `packages/<group>/` directory names, not package names.
  { file: 'scripts/gen-module-graph.ts', upstream: ['cordis'] },
  { file: 'scripts/gen-doc-graphs.ts', upstream: ['cordis'] },
  // `cordis/*` is the extensions event domain, not a package subpath. The
  // generated catalogs and every producer/consumer must preserve that wire id.
  { file: 'docs/event-producer-consumer.md', upstream: ['cordis'] },
  { file: 'docs/event-producer-consumer.zh.md', upstream: ['cordis'] },
  { file: 'docs/subsystems/extensions.md', upstream: ['cordis'] },
  { file: 'docs/subsystems/extensions.zh.md', upstream: ['cordis'] },
  { file: 'packages/api/remotes/src/remote-events.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-client-runner/src/client/index.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-client-runner/src/client/runtime.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-client-runner/tests/orchestrator.client.spec.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-client-runner/tests/plugin.client.spec.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-host-runner/src/index.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-host-runner/src/inspect-registry.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-host-runner/src/types.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-host-runner/tests/helpers.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-host-runner/tests/runner.spec.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/cordis-host-runner/tests/versioning.spec.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/tool-cordis/src/api-catalog.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/tool-cordis/src/providers.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/ui-cordis/src/client/index.ts', upstream: ['cordis'] },
  { file: 'packages/extensions/ui-cordis/src/client/inventory.ts', upstream: ['cordis'] },
  { file: 'scripts/gen-cordis-catalog.ts', upstream: ['cordis'] },
  // The UI locale namespace and input-trigger source id are product keys.
  { file: 'packages/client/ui-settings-plugin-inventory/src/client/PluginInventorySettingsTab.tsx', upstream: ['cordis'] },
  { file: 'packages/extensions/ui-cordis/src/client/CordisActionRow.tsx', upstream: ['cordis'] },
  { file: 'packages/extensions/ui-cordis/src/client/CordisDefineRow.tsx', upstream: ['cordis'] },
  { file: 'packages/extensions/ui-cordis/src/client/CordisPanel.tsx', upstream: ['cordis'] },
  { file: 'packages/extensions/ui-cordis/src/client/CordisRunRow.tsx', upstream: ['cordis'] },
  { file: 'packages/extensions/ui-cordis/src/client/locales.ts', upstream: ['cordis'] },
]

/** A string that must appear exactly `count` times once the rescope has run. */
interface PostCondition {
  readonly file: string
  readonly text: string
  readonly count: number
}

const POSTCONDITIONS: readonly PostCondition[] = [
  { file: 'vendor/cordis/package.json', text: '"name": "@deepseek-ai/cordis"', count: 1 },
  { file: 'vendor/hmr/package.json', text: '"name": "@deepseek-ai/cordis-plugin-hmr"', count: 1 },
  { file: 'scripts/cordis-walk.ts', text: '@deepseek-ai\\/cordis', count: 1 },
  { file: 'scripts/cordis-walk.ts', text: '!== \'@deepseek-ai/cordis\'', count: 1 },
  { file: 'scripts/gen-scoped-events.ts', text: '=== \'@deepseek-ai/cordis\'', count: 1 },
  { file: 'packages/typert/generator/src/analyzer.ts', text: '!== \'@deepseek-ai/cordis\'', count: 2 },
  { file: 'scripts/check-workspace-constraints.ts', text: '?.[\'@deepseek-ai/cordis\']', count: 2 },
  { file: 'packages/boot/app-boot/tsdown.config.ts', text: '[\'@deepseek-ai/cordis-plugin-include\']', count: 1 },
  { file: 'tsconfig.base.json', text: '"@deepseek-ai/cordis-plugin-loader": ["./vendor/loader/src"]', count: 1 },
  // The vendored README owns this required entry; reject its deletion or duplication.
  { file: 'vendor/README.md', text: '17. **`@deepseek-ai` rescope**', count: 1 },
  { file: 'knip.json', text: '@cordisjs', count: 0 },
  // The preset ids in this table are product data, not package names.
  { file: 'packages/client/ui-agent-preset/tests/locales.client.spec.ts', text: '[\'cordis\', \'presetCordisName\'', count: 1 },
  // The preset id the shipped composition documents to its own model.
  { file: 'apps/cli/config/agent-presets/cordis/agent.cordis.yml', text: 'The `cordis` agent preset', count: 1 },
  { file: 'apps/cli/config/agent-presets/cordis/agent.cordis.yml', text: 'corrupting the `cordis` preset', count: 1 },
  { file: 'packages/examples/acp-demo/tests/built-bin.e2e.ts', text: '\'cordis\', \'loader\', \'include\', \'timer\', \'hmr\', \'logger-console\',', count: 1 },
]

/**
 * Every exact edit, in application order. Each `find` is written against the
 * PRE-rename text because these run before the generic pass, so no `find` may
 * quote a neighbouring line the generic pass would rewrite.
 */
const EXACT_EDITS: readonly ExactEdit[] = [
  {
    id: 'cordis-walk-merge-head',
    file: 'scripts/cordis-walk.ts',
    find: 'const MERGE_HEAD = /declare module [\'"](?:cordis|\\.\\/context\\.ts)[\'"]/',
    replace: 'const MERGE_HEAD = /declare module [\'"](?:@deepseek-ai\\/cordis|\\.\\/context\\.ts)[\'"]/',
    expect: 1,
  },
  {
    id: 'constraints-manifest-lookup',
    file: 'scripts/check-workspace-constraints.ts',
    find: `    const peer = manifest.peerDependencies?.cordis
    const dev = manifest.devDependencies?.cordis

    if (!peer) errors.push(\`\${label}: cordis must be a peerDependency\`)
    if (!dev) errors.push(\`\${label}: cordis must also be a devDependency\`)
    if (peer && dev && peer !== dev) {
      errors.push(\`\${label}: cordis peer (\${peer}) and dev (\${dev}) ranges must match\`)`,
    replace: `    const peer = manifest.peerDependencies?.['@deepseek-ai/cordis']
    const dev = manifest.devDependencies?.['@deepseek-ai/cordis']

    if (!peer) errors.push(\`\${label}: @deepseek-ai/cordis must be a peerDependency\`)
    if (!dev) errors.push(\`\${label}: @deepseek-ai/cordis must also be a devDependency\`)
    if (peer && dev && peer !== dev) {
      errors.push(\`\${label}: @deepseek-ai/cordis peer (\${peer}) and dev (\${dev}) ranges must match\`)`,
    expect: 1,
  },
  {
    // The rescoped name is already covered by the `@deepseek-ai/.+` pattern beside it.
    id: 'knip-logger-console',
    file: 'knip.json',
    find: `      "ignoreDependencies": [
        "@cordisjs/plugin-logger-console",
        "@deepseek-ai/.+"
      ]
    },
    "packages/util/home": {`,
    replace: `      "ignoreDependencies": [
        "@deepseek-ai/.+"
      ]
    },
    "packages/util/home": {`,
    expect: 1,
  },
  {
    id: 'knip-bundle-base',
    file: 'knip.json',
    find: `    "packages/bundle/base": {
      "ignoreDependencies": [
        "@deepseek-ai/.+",
        "@cordisjs/.+"
      ]`,
    replace: `    "packages/bundle/base": {
      "ignoreDependencies": [
        "@deepseek-ai/.+"
      ]`,
    expect: 1,
  },

  {
    id: 'publication-set-scope-assertion',
    file: 'scripts/publish-npm-baseline.ts',
    find: '      if (!isVendored && !name.startsWith(\'@deepseek-ai/\')) {',
    replace: `      // Vendored packages are rescoped too (vendor/README.md), so publication
      // never carries an upstream name that would squat it on the registry.
      if (!name.startsWith('@deepseek-ai/')) {`,
    expect: 1,
  },
  {
    id: 'vendor-readme-preamble',
    file: 'vendor/README.md',
    find: 'All vendored packages are **renamed into the `@deepseek-ai` scope** (`cordis` → `@deepseek-ai/cordis`, `@cordisjs/plugin-<x>` → `@deepseek-ai/cordis-plugin-<x>`): every harness package declares `cordis` as a peer dependency, so publishing the harness publishes this framework layer too, and a publication under the upstream names would squat them on the registry. Directory names and upstream version numbers are deliberately unchanged, so the manifest below still reads as an upstream snapshot. `pnpm-workspace.yaml#linkWorkspacePackages` makes those preserved semver ranges resolve these pinned workspaces, including imports from built `lib/`.',
    replace: 'All vendored packages are **renamed into the `@deepseek-ai` scope** (`cordis` → `@deepseek-ai/cordis`, `@cordisjs/plugin-<x>` → `@deepseek-ai/cordis-plugin-<x>`): every harness package declares `cordis` as a peer dependency, so publishing the harness publishes this framework layer too, and a publication under the upstream names would squat them on the registry. Directory names and upstream version numbers are deliberately unchanged, so the manifest below still reads as an upstream snapshot. The root manifest\'s workspaces globs plus its `overrides` entries (`@deepseek-ai/cosmokit`, `@deepseek-ai/schemastery` → `workspace:*`) make those preserved semver ranges resolve these pinned sources, including imports from built `lib/`.',
    expect: 1,
  },
  {
    // The Schemastery note quotes the require() call whose argument renames.
    id: 'vendor-readme-schemastery-note',
    file: 'vendor/README.md',
    find: 'whose lazy `require(\'cosmokit\')` can race',
    replace: 'whose lazy `require(\'@deepseek-ai/cosmokit\')` can race',
    expect: 1,
  },
  {
    id: 'vendor-readme-table-head',
    file: 'vendor/README.md',
    find: '| Directory | npm name | Version | Upstream repo | Commit |\n|---|---|---|---|---|',
    replace: '| Directory | npm name | Upstream name | Version | Upstream repo | Commit |\n|---|---|---|---|---|---|',
    expect: 1,
  },
  {
    // A plain fence listing the bundle's mounted tree: a bare token, no quotes.
    id: 'agent-spine-demo-mounted-tree',
    file: 'packages/examples/agent-spine-demo/README.md',
    find: '@cordisjs/plugin-timer            timer service',
    replace: '@deepseek-ai/cordis-plugin-timer  timer service',
    expect: 1,
  },
  {
    id: 'agent-spine-demo-mounted-tree-zh',
    file: 'packages/examples/agent-spine-demo/README.zh.md',
    find: '@cordisjs/plugin-timer            timer service',
    replace: '@deepseek-ai/cordis-plugin-timer  timer service',
    expect: 1,
  },
  {
    // The root contract claimed vendored packages keep their upstream names.
    id: 'root-agents-vendored-name-contract',
    file: 'AGENTS.md',
    find: 'vendored packages keep upstream names and are `private: true`. `cordis` is a peerDependency (+ dev) of every harness package.',
    replace: 'vendored packages are rescoped ([mapping](docs/rescope.md)) and `private: true`. `@deepseek-ai/cordis` is a peerDependency (+ dev) of every harness package.',
    expect: 1,
  },
]

/** Whether one tracked path participates in the rescope at all. */
function rescopeTarget(file: string): boolean {
  if (file === 'scripts/rescope-vendor.ts') return false // these tables quote both names as data
  if (file.startsWith('.agents/notes/')) return false // notes record what was true when written
  // Recorded model payloads quote documentation verbatim, so they must mirror
  // the sources on disk — including the notes this rescope leaves alone.
  if (file.startsWith('scripts/snapshots/')) return false
  // The mapping documents state both names on purpose.
  if (file === 'docs/rescope.md' || file === 'docs/rescope.zh.md') return false
  if (file.endsWith('.i18n.yaml')) return false // blob-hash records, re-recorded by the pairing gate
  if (file === 'bun.lock') return false // regenerated by bun install
  if (/^vendor\/[^/]+\/(README\.md|LICENSE)$/.test(file)) return true // upstream files kept verbatim
  return EXTENSIONS.some(extension => file.endsWith(extension))
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?${}()|[\]\\]/g, '\\$&')
}

/** Rename direction for one run: apply maps upstream → scoped; reverse undoes it. */
function activeRenames(reverse: boolean): readonly { from: string; to: string }[] {
  const pairs = RENAMES.map(rename => ({ from: rename.upstream, to: rename.scoped }))
  return reverse ? pairs.map(pair => ({ from: pair.to, to: pair.from })).reverse() : pairs
}

/**
 * Rewrite only delimited, complete package-name tokens: a quote (or a YAML
 * `name: ` prefix) immediately left of the name and the matching quote —
 * optionally after a `/subpath` — immediately right. The scoped result never
 * rematches because its final segment is preceded by `/`.
 */
function rescopeTokens(text: string, renames: readonly { from: string; to: string }[], skipNames: readonly string[]): string {
  let out = text
  for (const rename of renames) {
    if (skipNames.includes(rename.from)) continue
    const pattern = new RegExp(`(?<=['"\`]|name: )${escapeRegExp(rename.from)}(?=(?:\\/[^'"\`\n]*)?['"\`])`, 'g')
    out = out.replace(pattern, rename.to)
  }
  return out
}

/**
 * Apply every exact edit registered for one file, honoring the direction. An
 * edit whose source text is present is performed; one already in its target
 * state counts as satisfied — that is what makes a second apply a no-op. Any
 * other count fails loud instead of being skipped.
 */
function applyExactEdits(file: string, text: string, reverse: boolean): { text: string; problems: string[] } {
  const problems: string[] = []
  let out = text
  for (const edit of EXACT_EDITS) {
    if (edit.file !== file) continue
    const source = reverse ? edit.replace : edit.find
    const target = reverse ? edit.find : edit.replace
    const sourceCount = out.split(source).length - 1
    const targetCount = out.split(target).length - 1
    if (sourceCount === edit.expect && targetCount === 0) {
      out = out.split(source).join(target)
    } else if (!(targetCount === edit.expect && sourceCount === 0)) {
      problems.push(`${edit.id}: expected source ×${String(edit.expect)} → target ×${String(edit.expect)}, found source ×${String(sourceCount)}, target ×${String(targetCount)}`)
    }
  }
  return { text: out, problems }
}

/** Transform one file's content, or undefined when nothing changes. */
function rescopeContent(
  file: string,
  text: string,
  renames: readonly { from: string; to: string }[],
  reverse: boolean,
): { text: string; problems: string[] } | undefined {
  const skipNames = GENERIC_SKIPS.find(entry => entry.file === file)?.upstream ?? []
  const edited = applyExactEdits(file, text, reverse)
  // Markdown is governed solely by its exact edits: prose and fences quote
  // upstream names legitimately (mapping tables, compatibility notes), so a
  // bare-token pass there would corrupt documented state.
  const tokenRenames = file.endsWith('.md') ? [] : renames
  const out = rescopeTokens(edited.text, tokenRenames, skipNames)
  return out === text && edited.problems.length === 0 ? undefined : { text: out, problems: edited.problems }
}

function collectTargets(): string[] {
  // Cached plus untracked-unignored: an unborn or partially staged index must
  // not silently narrow the rescope surface to zero files.
  const listed = execFileSync('git', ['ls-files', '-z', '--cached', '--others', '--exclude-standard'], { cwd: root })
    .toString().split('\0').filter(name => name !== '')
  return listed.filter(rescopeTarget).sort()
}

function main(): void {
  const args = process.argv.slice(2)
  const mode = args.includes('--apply') ? 'apply' : args.includes('--check') ? 'check' : 'dry'
  const reverse = args.includes('--reverse')
  const renames = activeRenames(reverse)
  const files = collectTargets()
  const pending: string[] = []
  const problems: string[] = []

  for (const file of files) {
    const path = resolve(root, file)
    if (!existsSync(path)) continue
    const current = readFileSync(path, 'utf8')
    const outcome = rescopeContent(file, current, renames, reverse)
    if (outcome === undefined) continue
    pending.push(file)
    for (const problem of outcome.problems) problems.push(`${file}: ${problem}`)
    if (mode === 'apply' && outcome.problems.length === 0) writeFileSync(path, outcome.text)
  }

  // Postconditions describe the post-rescope state, so they hold in every mode
  // except a reverse run, which leaves the pre-rescope state behind.
  if (!reverse && mode !== 'dry') {
    for (const condition of POSTCONDITIONS) {
      const path = resolve(root, condition.file)
      if (!existsSync(path)) {
        problems.push(`postcondition target missing: ${condition.file}`)
        continue
      }
      const count = readFileSync(path, 'utf8').split(condition.text).length - 1
      if (count !== condition.count) {
        problems.push(`${condition.file}: expected ${JSON.stringify(condition.text)} × ${String(condition.count)}, found ${String(count)}`)
      }
    }
  }

  console.log(`rescope-vendor: ${mode}${reverse ? ' --reverse' : ''} over ${String(files.length)} tracked files`)
  if (pending.length > 0) {
    if (mode === 'apply') {
      console.log(`rescope-vendor: rewrote ${String(pending.length)} file(s)`)
    } else {
      console.log(`rescope-vendor: ${String(pending.length)} file(s) would change:`)
      for (const file of pending) console.log(`  - ${file}`)
    }
  }
  for (const problem of problems) console.error(`rescope-vendor: ${problem}`)
  if (problems.length > 0 || (mode === 'check' && pending.length > 0)) process.exitCode = 1
}

main()
