import { execFileSync } from 'node:child_process'
import { delimiter } from 'node:path'
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { NcagOrchestrator } from '../src/index.ts'
import type { OrchestratorConfig } from '../src/index.ts'
import { ScriptedModel } from './scripted-model.ts'
import type { ScriptStep } from './scripted-model.ts'

/** A running orchestrator over a temporary repository and database. */
export interface Harness {
  readonly orchestrator: NcagOrchestrator
  readonly repository: string
  readonly databasePath: string
  readonly workspaceRoot: string
  readonly model: ScriptedModel
  /** Stop the orchestrator and remove every temporary directory. */
  cleanup(): Promise<void>
}

/** Resolve the git executable, refusing to guess when the PATH does not carry it. */
export function resolveGitExecutable(): string {
  const fromEnv = process.env.GIT_EXECUTABLE
  if (fromEnv !== undefined && fromEnv.length > 0) return fromEnv
  for (const directory of (process.env.PATH ?? '').split(delimiter)) {
    if (directory.length === 0) continue
    const candidate = join(directory, process.platform === 'win32' ? 'git.exe' : 'git')
    if (existsSync(candidate)) return candidate
  }
  throw new Error('ncag: git is not on PATH; set GIT_EXECUTABLE to its absolute path')
}

/** Create a temporary git repository holding one commit. */
function createRepository(repositoryPath: string): void {
  const git = resolveGitExecutable()
  mkdirSync(repositoryPath, { recursive: true })
  execFileSync(git, ['init', '-b', 'main'], { cwd: repositoryPath, stdio: 'ignore' })
  writeFileSync(join(repositoryPath, 'README.md'), '# scratch project\n')
  execFileSync(git, ['add', '-A'], { cwd: repositoryPath, stdio: 'ignore' })
  execFileSync(git, ['-c', 'user.name=NC-AG', '-c', 'user.email=ncag@localhost', 'commit', '-m', 'initial'], { cwd: repositoryPath, stdio: 'ignore' })
}

if (process.env.GIT_EXECUTABLE === undefined) {
  const resolved = resolveGitExecutable()
  process.env.GIT_EXECUTABLE = resolved
}

/**
 * Reopen the platform over the database and repository of a stopped harness,
 * which is a genuine process restart: no in-memory state survives.
 * @param previous - the harness whose orchestrator was stopped.
 * @returns a new harness over the same durable state.
 */
export async function restartHarness(previous: Harness): Promise<Harness> {
  const orchestrator = new NcagOrchestrator({
    config: {
      databasePath: previous.databasePath,
      workspaceRoot: previous.workspaceRoot,
      modelPricing: [{ provider: 'test-provider', model: 'test-model', inputPerMillion: 1, outputPerMillion: 2, tier: 'standard' }],
      preferredProviders: ['test-provider'],
      leaseTtlMs: 60_000,
      maxConcurrentAgents: 3,
      maxStepsPerTask: 8,
      maxOutputTokens: 1024,
      healthIntervalMs: 60_000,
      stallTimeoutMs: 600_000,
      repeatedToolThreshold: 4,
    },
    llm: previous.model,
    routeRegistry: null,
  })
  await previous.orchestrator.stop()
  await orchestrator.start()
  return {
    orchestrator,
    repository: previous.repository,
    databasePath: previous.databasePath,
    workspaceRoot: previous.workspaceRoot,
    model: previous.model,
    cleanup: async () => {
      await orchestrator.stop()
    },
  }
}

/**
 * Start one orchestrator over a fresh temporary project.
 * @param scripts - scripts keyed by agent system-prompt phrase.
 * @param config - configuration overrides applied to the default test configuration.
 * @returns the running harness.
 */
export async function startHarness(
  scripts: ReadonlyMap<string, readonly ScriptStep[]>,
  config: Partial<OrchestratorConfig> = {},
): Promise<Harness> {
  const root = mkdtempSync(join(tmpdir(), 'ncag-test-'))
  const repository = join(root, 'repository')
  const workspaceRoot = join(root, 'workspaces')
  createRepository(repository)
  const databasePath = join(root, 'platform.db')
  const model = new ScriptedModel(scripts)
  const orchestrator = new NcagOrchestrator({
    config: {
      databasePath,
      workspaceRoot,
      modelPricing: [{ provider: 'test-provider', model: 'test-model', inputPerMillion: 1, outputPerMillion: 2, tier: 'standard' }],
      preferredProviders: ['test-provider'],
      leaseTtlMs: 60_000,
      maxConcurrentAgents: 3,
      maxStepsPerTask: 8,
      maxOutputTokens: 1024,
      healthIntervalMs: 60_000,
      stallTimeoutMs: 600_000,
      repeatedToolThreshold: 4,
      ...config,
    },
    llm: model,
    routeRegistry: null,
  })
  await orchestrator.start()
  return {
    orchestrator,
    repository,
    databasePath,
    workspaceRoot,
    model,
    cleanup: async () => {
      await orchestrator.stop()
      rmSync(root, { recursive: true, force: true })
    },
  }
}
