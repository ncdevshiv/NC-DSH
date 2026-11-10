import { lstat, readFile, readdir, realpath, rm } from 'node:fs/promises'
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'
import { repositoryConfigHost } from './ts-project.ts'

const knownOrphanEntries = new Set(['node_modules', 'lib', '.typecheck'])

function isMissing(error: unknown): boolean {
  return error instanceof Error && 'code' in error && error.code === 'ENOENT'
}

async function exists(path: string): Promise<boolean> {
  try {
    await lstat(path)
    return true
  } catch (error) {
    if (isMissing(error)) return false
    throw error
  }
}

async function childDirectories(path: string): Promise<string[]> {
  try {
    const entries = await readdir(path, { withFileTypes: true })
    return entries.filter(entry => entry.isDirectory()).map(entry => join(path, entry.name))
  } catch (error) {
    if (isMissing(error)) return []
    throw error
  }
}

function repositoryPath(root: string, path: string): string {
  return relative(root, path).split(sep).join('/')
}

function parseConfig(configPath: string): ts.ParsedCommandLine {
  const parsed = ts.getParsedCommandLineOfConfigFile(configPath, {}, repositoryConfigHost)
  if (!parsed) throw new Error(`clean: cannot parse TypeScript config ${configPath}`)
  if (parsed.errors.length > 0) {
    throw new Error(parsed.errors.map(error => ts.flattenDiagnosticMessageText(error.messageText, '\n')).join('\n'))
  }
  return parsed
}

/** Plans and removes repository-owned build output without crossing the repository boundary. */
export class RepositoryCleaner {
  private readonly root: string

  constructor(root: string) {
    this.root = resolve(root)
  }

  /**
   * Remove generated build state and package directories containing only known residue.
   * @returns Repository-relative paths that were removed.
   */
  async clean(): Promise<string[]> {
    const targets = await this.plan()
    // Planning validates every target first, so an unsafe orphan prevents all deletion.
    for (const target of targets) await rm(target, { recursive: true, force: true })
    return targets.map(target => repositoryPath(this.root, target))
  }

  private async plan(): Promise<string[]> {
    const targets = new Set<string>()
    const unsafeOrphans: string[] = []
    const canonicalRoot = await realpath(this.root)

    await this.addIfPresent(targets, join(this.root, '.dsh-build'), canonicalRoot)
    await this.addIfPresent(targets, join(this.root, 'apps/desktop/.desktop-build'), canonicalRoot)

    // These checks cover legacy root-level incremental state emitted by older configs.
    await this.addIfPresent(targets, join(this.root, '.typecheck'), canonicalRoot)
    for (const entry of await readdir(this.root, { withFileTypes: true })) {
      if (entry.isFile() && entry.name.endsWith('.tsbuildinfo')) targets.add(join(this.root, entry.name))
    }
    await this.addIfPresent(
      targets,
      join(this.root, 'native/system/tsconfig.tsbuildinfo'),
      canonicalRoot,
    )

    // The root project-reference graph is the source of truth for live build targets.
    // Each emitting project declares lib/types as outDir; its parent lib also owns
    // the sibling runtime bundles, so the complete build output root is removed.
    const graph = this.projectGraph()
    for (const outputDirectory of graph.outputs) {
      await this.addIfPresent(targets, outputDirectory, canonicalRoot)
    }

    // Compiler output that landed inside a source tree is build residue as well:
    // each removed file is proven by its own source map to be emitted from the
    // sibling TypeScript source, which stays.
    for (const outputFile of await this.sourceTreeCompilerOutput(graph.sources)) {
      this.assertRepositoryTarget(outputFile)
      targets.add(outputFile)
    }

    for (const groupDirectory of await childDirectories(join(this.root, 'packages'))) {
      for (const packageDirectory of await childDirectories(groupDirectory)) {
        // A package.json marks a live package; its output was discovered from the
        // project graph above, and its package-local node_modules must be preserved.
        if (await exists(join(packageDirectory, 'package.json'))) {
          continue
        }

        // A manifest-less package directory is stale only when every remaining
        // entry is known generated residue; unknown files make the whole clean fail.
        const entries = await readdir(packageDirectory)
        const unknown = entries.filter(entry => !knownOrphanEntries.has(entry) && !entry.endsWith('.tsbuildinfo'))
        if (unknown.length > 0) {
          unsafeOrphans.push(...unknown.map(entry => repositoryPath(this.root, join(packageDirectory, entry))))
        } else {
          await this.addIfPresent(targets, packageDirectory, canonicalRoot)
        }
      }
    }

    if (unsafeOrphans.length > 0) {
      throw new Error([
        'clean: refusing to remove package directories without package.json; unknown entries remain:',
        ...unsafeOrphans.sort().map(path => `  ${path}`),
      ].join('\n'))
    }

    return [...targets].sort()
  }

  private projectGraph(): { outputs: string[]; sources: string[] } {
    const outputs = new Set<string>()
    const sources = new Set<string>()
    const pending = [join(this.root, 'tsconfig.json')]
    const visited = new Set<string>()
    const nativeEntryOutput = join(this.root, 'native/system/packages/entry/lib')

    while (pending.length > 0) {
      const nextConfigPath = pending.pop()
      if (nextConfigPath === undefined) break
      const configPath = resolve(nextConfigPath)
      if (visited.has(configPath)) continue
      visited.add(configPath)

      const parsed = parseConfig(configPath)
      for (const fileName of parsed.fileNames) {
        if (fileName.endsWith('.ts') || fileName.endsWith('.tsx')) sources.add(fileName)
      }
      if (parsed.options.outDir !== undefined) {
        const typesDirectory = resolve(parsed.options.outDir)
        const outputDirectory = basename(typesDirectory) === 'types'
          ? dirname(typesDirectory)
          : typesDirectory === nativeEntryOutput
            ? typesDirectory
            : undefined
        if (outputDirectory === undefined) {
          throw new Error(`clean: expected TypeScript outDir to end in /types: ${repositoryPath(this.root, typesDirectory)}`)
        }
        this.assertRepositoryTarget(outputDirectory)
        outputs.add(outputDirectory)
      }

      for (const reference of parsed.projectReferences ?? []) {
        pending.push(ts.resolveProjectReferencePath(reference))
      }
    }

    return { outputs: [...outputs], sources: [...sources] }
  }

  /**
   * Find compiler output that landed next to a TypeScript source.
   *
   * A file qualifies only when its own source map names the sibling TypeScript
   * source it was emitted from, so a hand-written JavaScript file, a file whose
   * map names another input, and a file whose source is gone all stay.
   * @param sources - The TypeScript sources of the project graph.
   * @returns The residue files and their source maps.
   */
  private async sourceTreeCompilerOutput(sources: string[]): Promise<string[]> {
    const residue: string[] = []
    for (const source of sources) {
      const base = source.replace(/\.tsx?$/, '')
      for (const output of [`${base}.js`, `${base}.d.ts`]) {
        if (!await this.provesCompilerOutput(output)) continue
        residue.push(output, `${output}.map`)
      }
    }
    return residue
  }

  /**
   * Check whether a file's source map proves the file was emitted from a
   * sibling TypeScript source.
   * @param outputPath - The candidate output file.
   * @returns `true` when the file has a source map naming a sibling source.
   */
  private async provesCompilerOutput(outputPath: string): Promise<boolean> {
    const mapPath = `${outputPath}.map`
    if (!await exists(mapPath)) return false
    let map: unknown
    try {
      map = JSON.parse(await readFile(mapPath, 'utf8'))
    } catch {
      return false
    }
    const sources = (map as { sources?: unknown }).sources
    if (!Array.isArray(sources)) return false
    const directory = dirname(outputPath)
    for (const entry of sources) {
      if (typeof entry !== 'string') continue
      const source = resolve(directory, entry)
      // TypeScript reports program files with forward slashes; compare resolved paths.
      if (resolve(dirname(source)) !== resolve(directory)) continue
      if (!source.endsWith('.ts') && !source.endsWith('.tsx')) continue
      if (await exists(source)) return true
    }
    return false
  }

  private assertRepositoryTarget(path: string): void {
    this.assertDescendant(this.root, path, path)
  }

  private assertDescendant(root: string, path: string, displayPath: string): void {
    const repositoryRelative = relative(root, path)
    if (repositoryRelative === '' || repositoryRelative === '..' || repositoryRelative.startsWith(`..${sep}`) || isAbsolute(repositoryRelative)) {
      throw new Error(`clean: refusing deletion target outside repository: ${displayPath}`)
    }
  }

  private async addIfPresent(targets: Set<string>, path: string, canonicalRoot: string): Promise<void> {
    // Missing outputs are normal on a clean checkout; only existing paths become deletion targets.
    if (!await exists(path)) return
    // Resolve the parent rather than the final entry: rm unlinks a final symlink,
    // but a symlink in an ancestor would make deletion cross the repository boundary.
    const canonicalParent = await realpath(dirname(path))
    this.assertDescendant(canonicalRoot, join(canonicalParent, basename(path)), path)
    targets.add(path)
  }
}

const scriptPath = fileURLToPath(import.meta.url)
if (process.argv[1] !== undefined && resolve(process.argv[1]) === scriptPath) {
  try {
    const removed = await new RepositoryCleaner(resolve(dirname(scriptPath), '..')).clean()
    if (removed.length === 0) {
      console.log('clean: already clean')
    } else {
      console.log(`clean: removed ${removed.length} paths`)
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : error)
    process.exitCode = 1
  }
}
