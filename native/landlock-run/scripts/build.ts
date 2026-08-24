/**
 * Build every native tool this host can build, into its per-platform
 * package.
 *
 * Targets are derived from the checked-in matrix: each
 * `packages/<name>/prebuilds.json` whose `platform` matches this host names
 * the binaries to produce; the TOOLS table below maps each `tool` to its Rust
 * crate. Builds are NATIVE-ONLY — each Linux architecture compiles its own
 * binary with cargo against the bundled static musl target
 * (`rustup target add <triple>`; rustc ships self-contained CRT objects for
 * the musl targets and uses the host C compiler only as the linker driver).
 * The result runs on glibc and musl distros alike, with no loader or libc
 * expectations on the consumer host. CI's per-arch runners are the builders
 * of record; no cross toolchain exists here on purpose: native runners
 * replace it, and the audit surface is the reviewed launcher source plus the
 * CI job that built the binary.
 *
 * Binaries land in `packages/<name>/bin/` — git-ignored (workspace
 * `.gitignore`), packed into the platform package's npm tarball behind its
 * `prepack` gate (`scripts/verify-launcher-binary.mjs`).
 *
 * Run: `bun run build:native` (Linux with cargo on PATH and the musl std
 * target installed: `rustup target add x86_64-unknown-linux-musl` or
 * `aarch64-unknown-linux-musl`). Non-Linux hosts fail fast — no platform
 * package exists for them to build.
 */
import { spawnSync } from 'node:child_process'
import { chmodSync, copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync } from 'node:fs'
import { basename, dirname, join, resolve } from 'node:path'

/** Each native tool's crate manifest, keyed by the `tool` field in prebuilds.json. */
const TOOLS: Record<string, { manifestPath: string }> = {
  'landlock-run': { manifestPath: 'packages/entry/native/Cargo.toml' },
}

/** The Rust target triple this build produces for each supported host arch. */
const RUST_TARGETS: Record<string, string> = {
  x64: 'x86_64-unknown-linux-musl',
  arm64: 'aarch64-unknown-linux-musl',
}

const repoRoot = resolve(import.meta.dirname, '..')

if (process.platform !== 'linux') {
  console.error(`build: native tools are built natively per Linux architecture (no cross toolchain) — nothing to build on ${process.platform}. CI's per-arch runners build and rehearse every platform package.`)
  process.exit(1)
}
const rustTarget = RUST_TARGETS[process.arch]
if (rustTarget === undefined) {
  console.error(`build: no Rust musl target is mapped for ${process.arch} — extend RUST_TARGETS together with the package matrix.`)
  process.exit(1)
}
const hostPlatform = `linux-${process.arch}`

/** This host's platform packages, from the checked-in matrix. */
const targets: { packageDir: string; tool: string; binaryPath: string; kind: string }[] = []
const packagesRoot = join(repoRoot, 'packages')
for (const name of readdirSync(packagesRoot).sort()) {
  const prebuildsFile = join(packagesRoot, name, 'prebuilds.json')
  if (!existsSync(prebuildsFile)) continue
  const prebuilds = JSON.parse(readFileSync(prebuildsFile, 'utf8')) as {
    platform: string
    binaries: { tool: string; kind: string; path: string }[]
  }
  if (prebuilds.platform !== hostPlatform) continue
  for (const binary of prebuilds.binaries) {
    targets.push({ packageDir: join(packagesRoot, name), tool: binary.tool, binaryPath: binary.path, kind: binary.kind })
  }
}
if (targets.length === 0) {
  console.error(`build: no platform package declares binaries for ${hostPlatform} — supported platforms are the packages/*/prebuilds.json "platform" values.`)
  process.exit(1)
}

for (const target of targets) {
  const tool = TOOLS[target.tool]
  if (tool === undefined) {
    console.error(`build: prebuilds.json names unknown tool "${target.tool}" — add it to the TOOLS table in scripts/build.ts.`)
    process.exit(1)
  }
  if (target.kind !== 'static-musl') {
    console.error(`build: unknown binary kind "${target.kind}" — the only toolchain here is static musl.`)
    process.exit(1)
  }
  const binary = join(target.packageDir, target.binaryPath)
  mkdirSync(dirname(binary), { recursive: true })

  // Static against the bundled musl: self-contained, no loader/libc
  // expectations on the consumer host. Release profile pins lto, panic=abort,
  // and symbol stripping so a new toolchain default cannot quietly change the
  // shipped artifact's shape.
  const result = spawnSync('cargo', [
    'build', '--release', '--target', rustTarget,
    '--manifest-path', join(repoRoot, tool.manifestPath),
  ], { stdio: ['ignore', 'inherit', 'inherit'] })
  if (result.error !== undefined || result.status !== 0) {
    console.error('build: cargo failed' +
      (result.error ? ` (${result.error.message} — is cargo installed with the ${rustTarget} target? \`rustup target add ${rustTarget}\`)` : ''))
    process.exit(1)
  }
  const built = join(repoRoot, tool.manifestPath, '..', 'target', rustTarget, 'release', 'landlock-run')
  copyFileSync(built, binary)
  // copyFileSync does not preserve the executable bit; pack-time gates assert it.
  chmodSync(binary, 0o755)
  console.log(`build: built ${basename(target.packageDir)}/${target.binaryPath}`)
}
