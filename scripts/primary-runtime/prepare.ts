/** Prepare pinned, relocatable script interpreters without installing into the build host. */

import { execFileSync } from 'node:child_process'
import { createHash, randomUUID } from 'node:crypto'
import { cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { cp } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { parseArgs } from 'node:util'
import extractZip from 'extract-zip'
import { x as extractTar } from 'tar'
import { copyPrimaryRuntime, parsePrimaryRuntime, workspaceDependencyPaths, type PrimaryRuntimeManifest } from '@deepseek-ai/dsh-tool-workspace-dependencies'
import lock from './lock.json' with { type: 'json' }
import { prunePrimaryRuntimePythonTests } from './prune-python-tests.ts'

/**
 * Download or reuse an archive only when its bytes match the release lock.
 * @param url - Locked archive URL.
 * @param sha256 - Expected hexadecimal digest for the selected algorithm.
 * @param cache - Download cache directory.
 * @param algorithm - Digest algorithm; npm archives use SHA-512.
 * @returns Verified local archive path.
 */
export async function downloadPrimaryRuntimeAsset(url: string, sha256: string, cache: string, algorithm: 'sha256' | 'sha512' = 'sha256'): Promise<string> {
  mkdirSync(cache, { recursive: true })
  const destination = join(cache, sha256)
  let bytes: Buffer
  let cached = true
  try { bytes = readFileSync(destination) } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error
    cached = false
    const timeout = Number(process.env.DSH_RESOURCE_DOWNLOAD_TIMEOUT_MS ?? '300000')
    if (!Number.isSafeInteger(timeout) || timeout < 1 || timeout > 2_147_483_647) throw new Error('DSH_RESOURCE_DOWNLOAD_TIMEOUT_MS must be an integer from 1 to 2147483647')
    const response = await fetch(url, { signal: AbortSignal.timeout(timeout) })
    if (!response.ok) throw new Error(`primary runtime download: ${String(response.status)} ${url}`)
    bytes = Buffer.from(await response.arrayBuffer())
  }
  if (createHash(algorithm).update(bytes).digest('hex') !== sha256) {
    rmSync(destination, { force: true })
    throw new Error(`primary runtime download: checksum mismatch for ${url}`)
  }
  if (cached) return destination
  const temporary = join(cache, `.${randomUUID()}`)
  try {
    writeFileSync(temporary, bytes)
    renameSync(temporary, destination)
  } finally {
    rmSync(temporary, { force: true })
  }
  return destination
}

/** Pinned npm tarball, without dependency resolution or lifecycle scripts. */
export interface RuntimeNpmArchive {
  /** Exact published package name. */
  readonly name: string
  /** Exact published version. */
  readonly version: string
  /** Registry tarball URL. */
  readonly url: string
  /** npm SHA-512 integrity. */
  readonly integrity: string
}

/**
 * Download and unpack one locked npm package without running its scripts.
 * @param artifact - Published tarball identity and integrity.
 * @param destination - Empty package directory.
 * @param cache - Verified archive cache.
 * @returns Resolves after package identity validation.
 */
export async function downloadRuntimeNpmPackage(artifact: RuntimeNpmArchive, destination: string, cache: string): Promise<void> {
  if (!/^sha512-[A-Za-z0-9+/]+={0,2}$/u.test(artifact.integrity)) throw new Error(`runtime download: invalid integrity for ${artifact.name}`)
  const checksum = Buffer.from(artifact.integrity.slice(7), 'base64').toString('hex')
  const archive = await downloadPrimaryRuntimeAsset(artifact.url, checksum, cache, 'sha512')
  mkdirSync(destination, { recursive: true })
  await extractTar({ file: archive, cwd: destination, strip: 1 })
  const installed = JSON.parse(readFileSync(join(destination, 'package.json'), 'utf8')) as { name?: string; version?: string }
  if (installed.name !== artifact.name || installed.version !== artifact.version) throw new Error(`runtime download: package identity mismatch for ${artifact.name}`)
}

/**
 * Prepare the locked standalone Node independently of Python and bun.
 * @param target - Target interpreter platform.
 * @param destination - Directory receiving bin/node and LICENSE.
 * @param cache - Verified archive cache.
 * @returns Resolves after extracting the interpreter.
 */
export async function downloadNodeRuntime(target: PrimaryRuntimeTarget, destination: string, cache: string): Promise<void> {
  const artifact = lock.targets[target]
  const filename = `node-v${lock.nodeVersion}-${artifact.nodeArchive}`
  const archive = await downloadPrimaryRuntimeAsset(`https://nodejs.org/dist/v${lock.nodeVersion}/${filename}`, artifact.nodeSha256, cache)
  const staging = mkdtempSync(join(tmpdir(), 'dsh-node-'))
  try {
    await extractNodeArchive(target, archive, staging)
    const source = join(staging, filename.replace(/\.(?:zip|tar\.gz)$/u, ''))
    mkdirSync(join(destination, 'bin'), { recursive: true })
    cpSync(join(source, ...(target === 'win-x64' ? ['node.exe'] : ['bin', 'node'])),
      join(destination, 'bin', target === 'win-x64' ? 'node.exe' : 'node'))
    cpSync(join(source, 'LICENSE'), join(destination, 'LICENSE'))
  } finally {
    rmSync(staging, { recursive: true, force: true })
  }
}

/**
 * Extract the Node.js archive for one target.
 * Windows archives are Zip; Windows' bundled bsdtar extracts them reliably,
 * while `extract-zip` leaves its promise unsettled on Node 26. Other build hosts
 * keep the maintained extractor.
 * @param target - Target whose Node.js archive is unpacked.
 * @param archive - Verified archive path.
 * @param staging - Directory receiving the extracted tree.
 */
async function extractNodeArchive(
  target: keyof typeof lock.targets,
  archive: string,
  staging: string,
): Promise<void> {
  if (target !== 'win-x64') {
    await extractTar({ file: archive, cwd: staging })
    return
  }
  await extractZipArchive(archive, staging)
}

/**
 * Extract a Zip archive.
 * Windows' bundled bsdtar extracts these reliably, while `extract-zip` leaves
 * its promise unsettled on Node 26. Other build hosts keep the maintained
 * extractor.
 * @param archive - Verified Zip archive path.
 * @param destination - Directory receiving the extracted tree.
 */
async function extractZipArchive(archive: string, destination: string): Promise<void> {
  if (process.platform === 'win32') {
    const systemRoot = process.env.SystemRoot ?? 'C:\\Windows'
    execFileSync(join(systemRoot, 'System32', 'tar.exe'), ['-xf', archive, '-C', destination], { stdio: 'pipe' })
    return
  }
  await extractZip(archive, { dir: destination })
}

async function pythonArchive(target: keyof typeof lock.targets, cache: string): Promise<string> {
  const artifact = lock.targets[target]
  const filename = `cpython-${lock.pythonVersion}+${lock.pythonRelease}-${artifact.pythonTarget}-install_only_stripped.tar.gz`
  return downloadPrimaryRuntimeAsset(`https://github.com/astral-sh/python-build-standalone/releases/download/${lock.pythonRelease}/${encodeURIComponent(filename)}`, artifact.pythonSha256, cache)
}

/**
 * Identify the inputs that assemble one target's payload, excluding unrelated target locks.
 * @param target - Runtime target whose archives are installed.
 * @param runtimeLock - Locked interpreter and wheel inputs.
 * @param bunVersion - Package-manager version copied into the payload.
 * @returns SHA-256 payload identity for installation reuse.
 */
export function primaryRuntimePayloadDigest(
  target: keyof typeof lock.targets, runtimeLock: typeof lock, bunVersion: string | undefined,
): string {
  const { pythonVersion, pythonRelease, nodeVersion, wheels, pythonPackages } = runtimeLock
  // Identity preserves key order within the selected target, wheel records and distribution map, plus wheel-entry order.
  // Bump format when extraction or assembly changes payload bytes without changing locked inputs.
  return createHash('sha256').update(JSON.stringify({
    format: 7, target, pythonVersion, pythonRelease, nodeVersion: bunVersion === undefined ? undefined : nodeVersion,
    artifact: runtimeLock.targets[target], wheels, pythonPackages, bun: bunVersion,
  })).digest('hex')
}

/**
 * Unpack a locked library wheel, retaining auxiliary scripts in its distribution data directory.
 * @param archive - Hash-verified wheel archive.
 * @param destination - Absolute site-packages directory.
 * @returns Resolves after extraction without command wrappers; rejects other wheel installation schemes.
 */
export async function unpackPrimaryRuntimeWheel(archive: string, destination: string): Promise<void> {
  await extractZipArchive(archive, destination)
  for (const entry of readdirSync(destination, { withFileTypes: true })) {
    if (!entry.isDirectory() || !entry.name.endsWith('.data')) continue
    for (const scheme of readdirSync(join(destination, entry.name))) {
      if (scheme !== 'scripts') {
        throw new Error(`primary runtime: wheel requires unsupported installation paths: ${entry.name}/${scheme}`)
      }
    }
  }
}

/**
 * Copy the skill package's complete asset tree to ordinary filesystem resources.
 * @param source - The package's assets directory.
 * @param destination - External Office skill resource directory.
 * @returns Resolves after replacing the external assets with the complete package tree.
 */
export async function prepareOfficeSkillAssets(source: string, destination: string): Promise<void> {
  rmSync(destination, { recursive: true, force: true })
  await cp(source, destination, { recursive: true, dereference: true })
}

/**
 * JavaScript entry that launches bun's native executable.
 * npm's bun package ships only platform binaries, while Desktop launches the
 * package manager as a Node script; this entry preserves that launch contract.
 */
const BUN_LAUNCHER = `#!/usr/bin/env node
import { spawn } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const executable = join(dirname(fileURLToPath(import.meta.url)), process.platform === 'win32' ? 'bun.exe' : 'bun')
const child = spawn(executable, process.argv.slice(2), { stdio: 'inherit' })
child.once('error', (error) => { console.error(error.message); process.exit(1) })
child.once('exit', (code) => { process.exitCode = code ?? 1 })
`

/**
 * Ensure the copied bun package exposes the JavaScript entry Desktop launches.
 * @param bunPackage - Copied bun package directory.
 */
function ensureBunJavaScriptEntry(bunPackage: string): void {
  const bin = join(bunPackage, 'bin')
  if (existsSync(join(bin, 'bun.mjs')) || existsSync(join(bin, 'bun.cjs'))) return
  try {
    writeFileSync(join(bin, 'bun.mjs'), BUN_LAUNCHER)
  } catch (error) {
    throw new Error(`primary runtime: cannot write bun JavaScript entry in ${bin}: ${error instanceof Error ? error.message : String(error)}`)
  }
}

/**
 * Locate the installed bun package manifest by walking the workspace's `node_modules`.
 * `require.resolve('bun')` names Bun's builtin module when preparation runs under Bun,
 * so it cannot find the npm package that supplies the interpreter copied into the payload.
 * @returns Absolute path to the bun package manifest.
 * @throws If no installed bun package is reachable from this module.
 */
function bunPackageManifest(): string {
  for (let directory = import.meta.dirname; ; directory = dirname(directory)) {
    const candidate = join(directory, 'node_modules', 'bun', 'package.json')
    if (existsSync(candidate)) return candidate
    if (dirname(directory) === directory) break
  }
  throw new Error('primary runtime: cannot locate the bun package; install workspace dependencies')
}

/** A locked interpreter and wheel target. */
export type PrimaryRuntimeTarget = keyof typeof lock.targets

/** Resource preparation inputs shared by builds and explicit SDK downloads. */
export interface PreparePrimaryRuntimeOptions {
  /** Target whose archives and wheels are downloaded. */
  readonly target: PrimaryRuntimeTarget
  /** Resource directory receiving primary-runtime/ and office-skills/. */
  readonly output: string
  /** SHA-256-addressed archive cache. */
  readonly cache: string
  /** Carrier release recorded in the legacy desktopVersion manifest field. */
  readonly version: string
  /** Locked npm bun tarball for installed carriers; build hosts otherwise copy their installed bun. */
  readonly bunArchive?: RuntimeNpmArchive
  /** Ordinary filesystem Office assets; build hosts otherwise resolve the workspace package. */
  readonly skillSource?: string
  /** Omit Node.js and bun for carriers providing only Python. */
  readonly pythonOnly?: boolean
}

/**
 * Materialize locked interpreters, libraries and Office resources without executing target code.
 * @param options - Explicit target and carrier-owned output locations.
 * @returns Resolves after the complete payload and skills have been copied to the output directory.
 */
export async function preparePrimaryRuntime(options: PreparePrimaryRuntimeOptions): Promise<void> {
  const { target } = options
  const paths = { runtime: resolve(options.output), downloads: resolve(options.cache) }
  mkdirSync(paths.runtime, { recursive: true })
  mkdirSync(paths.downloads, { recursive: true })
  const staging = mkdtempSync(join(tmpdir(), 'dsh-primary-'))
  try {
    const output = join(staging, 'payload')
    const dependencies = join(output, 'dependencies')
    mkdirSync(dependencies, { recursive: true })
    let bunVersion: string | undefined
    if (!options.pythonOnly) {
      const node = join(dependencies, 'node')
      await downloadNodeRuntime(target, node, paths.downloads)
      mkdirSync(join(node, 'node_modules'))
      writeFileSync(join(node, 'node_modules', 'README.txt'), 'Reserved for bundled Node packages. bun uses its default installation directories.\n')
      if (options.bunArchive !== undefined) {
        bunVersion = options.bunArchive.version
        await downloadRuntimeNpmPackage(options.bunArchive, join(dependencies, 'bun'), paths.downloads)
        ensureBunJavaScriptEntry(join(dependencies, 'bun'))
      } else {
        const bunManifest = bunPackageManifest()
        bunVersion = (JSON.parse(readFileSync(bunManifest, 'utf8')) as { version: string }).version
        await cp(dirname(bunManifest), join(dependencies, 'bun'), { recursive: true, dereference: true })
        ensureBunJavaScriptEntry(join(dependencies, 'bun'))
      }
    }
    await extractTar({ file: await pythonArchive(target, paths.downloads), cwd: dependencies })
    const manifest: PrimaryRuntimeManifest = {
      desktopVersion: options.version,
      platform: target === 'win-x64' ? 'win32' : target.startsWith('linux-') ? 'linux' : 'darwin',
      arch: target.endsWith('-arm64') ? 'arm64' : 'x64',
      payloadDigest: primaryRuntimePayloadDigest(target, lock, bunVersion),
      python: lock.pythonVersion,
      ...(bunVersion === undefined ? {} : { node: lock.nodeVersion, bun: bunVersion }),
      pythonPackages: lock.pythonPackages,
    }
    const entries = workspaceDependencyPaths(output, manifest)
    for (const wheel of [...lock.targets[target].wheels, ...lock.wheels]) {
      await unpackPrimaryRuntimeWheel(await downloadPrimaryRuntimeAsset(wheel.url, wheel.sha256, paths.downloads), entries.pythonPackages)
    }
    prunePrimaryRuntimePythonTests(entries.pythonPackages)
    writeFileSync(join(output, 'runtime.json'), `${JSON.stringify(manifest, undefined, 2)}\n`)
    const destination = join(paths.runtime, 'primary-runtime')
    rmSync(destination, { recursive: true, force: true })
    await copyPrimaryRuntime(output, destination, manifest)
  } finally {
    rmSync(staging, { recursive: true, force: true })
  }
  const assets = options.skillSource ?? join(dirname(createRequire(import.meta.url).resolve('@deepseek-ai/dsh-skill-office/package.json')), 'assets')
  await prepareOfficeSkillAssets(assets,
    join(paths.runtime, 'office-skills'))
}

/**
 * Execute the native payload's interpreters, package manager and Python libraries.
 * @param root - Final payload directory, including any platform signatures.
 * @param environment - Scrubbed subprocess environment; defaults to excluding credential-shaped names.
 */
export function smokePrimaryRuntime(root: string, environment: NodeJS.ProcessEnv = Object.fromEntries(
  Object.entries(process.env).filter(([name]) => !/(?:KEY|SECRET|TOKEN|PASSWORD)/iu.test(name)),
)): void {
  const manifest = parsePrimaryRuntime(JSON.parse(readFileSync(join(root, 'runtime.json'), 'utf8')))
  if (manifest.platform !== process.platform || manifest.arch !== process.arch) return
  if (Object.keys(manifest.pythonPackages).length === 0) throw new Error('primary runtime: missing Python distribution versions; prepare the payload before running its smoke checks.')
  const entries = workspaceDependencyPaths(root, manifest)
  if (manifest.platform !== 'win32') {
    const interpreter = `python${manifest.python.split('.').slice(0, 2).join('.')}`
    for (const name of ['python', 'python3']) {
      const path = join(dirname(entries.python), name)
      if (!lstatSync(path).isSymbolicLink() || readlinkSync(path) !== interpreter) {
        throw new Error(`primary runtime: Python commands must be relative aliases of ${interpreter}`)
      }
    }
    if (!lstatSync(join(dirname(entries.python), interpreter)).isFile()) throw new Error('primary runtime: versioned Python must be a regular file')
  }
  const options = { stdio: 'inherit', timeout: 120_000, env: environment } as const
  execFileSync(entries.python, ['-I', '-B', '-c', 'import decimal, xml.parsers.expat, lzma, uuid, numpy, pandas; assert numpy.arange(4).sum() == 6; assert pandas.DataFrame({"n": [1, 2]}).n.sum() == 3'], options)
  execFileSync(entries.python, ['-I', '-B', join(import.meta.dirname, 'smoke.py'), JSON.stringify(manifest.pythonPackages),
    manifest.python], options)
  execFileSync(entries.python, ['-I', '-B', '-m', 'pip', 'check'], options)
  if (entries.node !== undefined) execFileSync(entries.node, ['-e', `if (process.versions.node !== ${JSON.stringify(manifest.node)}) process.exit(1)`], options)
  if (entries.bun !== undefined && entries.node !== undefined) execFileSync(entries.node, [entries.bun, '--version'], options)
}

if (import.meta.main) {
  const { values } = parseArgs({ options: {
    target: { type: 'string' }, output: { type: 'string' }, cache: { type: 'string' },
    'python-only': { type: 'boolean', default: false },
  } })
  if (!values.target || !Object.hasOwn(lock.targets, values.target) || !values.output) {
    throw new Error(`Usage: bun run prepare:primary-runtime --target <${Object.keys(lock.targets).join('|')}> --output <directory> [--cache <directory>] [--python-only]`)
  }
  const { version } = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8')) as { version: string }
  const output = resolve(values.output)
  await preparePrimaryRuntime({
    target: values.target as PrimaryRuntimeTarget, output,
    cache: values.cache ?? join(tmpdir(), 'dsh-primary-runtime-downloads'), version,
    pythonOnly: values['python-only'],
  })
  smokePrimaryRuntime(join(output, 'primary-runtime'))
}
