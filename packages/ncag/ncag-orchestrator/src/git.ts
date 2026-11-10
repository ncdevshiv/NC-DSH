/**
 * Git workspace isolation: one worktree and branch per agent, change-set
 * submission, integration validation and rollback over real `git` commands.
 *
 * Every command runs with an explicit argv array and an explicit working
 * directory; a non-zero exit is reported with its captured output, never
 * silently ignored.
 * @module @deepseek-ai/dsh-ncag-orchestrator/git
 */

import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'

/** Result of one git command. */
export interface GitResult {
  readonly ok: boolean
  readonly stdout: string
  readonly stderr: string
  readonly exitCode: number
}

/** One isolated workspace created for an agent. */
export interface WorkspaceHandle {
  readonly path: string
  readonly branch: string
  readonly baseCommit: string
}

/** One submitted change set ready for integration. */
export interface SubmittedChange {
  readonly branch: string
  readonly baseCommit: string
  readonly headCommit: string
  readonly changedFiles: readonly string[]
}

/** Outcome of one integration attempt. */
export interface IntegrationResult {
  readonly status: 'MERGED' | 'CONFLICT' | 'REJECTED'
  readonly commit: string | null
  readonly report: readonly string[]
}

/**
 * Resolve the git executable. `git` is resolved from PATH by the operating system;
 * a deployment whose PATH does not carry git sets GIT_EXECUTABLE instead of
 * silently losing every workspace operation.
 * @returns the executable name or absolute path.
 */
export function gitExecutable(): string {
  const override = process.env.GIT_EXECUTABLE
  return override !== undefined && override.length > 0 ? override : 'git'
}

/**
 * Run one git command.
 * @param args - argv after the executable.
 * @param cwd - directory the command runs in.
 * @returns the captured outcome; a failure is a result, not an exception.
 */
export function git(args: readonly string[], cwd: string): GitResult {
  try {
    const stdout = execFileSync(gitExecutable(), [...args], { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })
    return { ok: true, stdout, stderr: '', exitCode: 0 }
  } catch (error: unknown) {
    const failure = error as { stdout?: string; stderr?: string; status?: number }
    return { ok: false, stdout: failure.stdout ?? '', stderr: failure.stderr ?? '', exitCode: typeof failure.status === 'number' ? failure.status : 1 }
  }
}

/**
 * Run one git command that must succeed.
 * @param args - argv after `git`.
 * @param cwd - directory the command runs in.
 * @returns trimmed stdout.
 * @throws Error carrying the captured output when the command fails.
 */
export function gitStrict(args: readonly string[], cwd: string): string {
  const result = git(args, cwd)
  if (!result.ok) {
    throw new Error(`ncag: git ${args.join(' ')} failed in ${cwd}: ${(result.stderr || result.stdout).trim().slice(0, 500)}`)
  }
  return result.stdout.trim()
}

/**
 * Read the current commit of one repository.
 * @param repositoryPath - repository root.
 * @param ref - ref to resolve, default HEAD.
 * @returns the commit sha.
 */
export function resolveCommit(repositoryPath: string, ref = 'HEAD'): string {
  return gitStrict(['rev-parse', ref], repositoryPath)
}

/**
 * Whether one path is inside a git work tree.
 * @param repositoryPath - candidate repository root.
 * @returns whether git recognizes the directory as a work tree.
 */
export function isRepository(repositoryPath: string): boolean {
  return git(['rev-parse', '--is-inside-work-tree'], repositoryPath).ok
}

/**
 * Create an isolated worktree and branch for one agent.
 * @param repositoryPath - repository root.
 * @param worktreePath - absolute path of the new worktree.
 * @param branch - new branch name.
 * @param baseBranch - branch to branch from.
 * @returns the created workspace.
 */
export function createWorkspace(repositoryPath: string, worktreePath: string, branch: string, baseBranch: string): WorkspaceHandle {
  if (!existsSync(repositoryPath)) throw new Error(`ncag: repository ${repositoryPath} does not exist`)
  const baseCommit = resolveCommit(repositoryPath, baseBranch)
  mkdirSync(dirname(worktreePath), { recursive: true })
  if (existsSync(worktreePath)) {
    throw new Error(`ncag: worktree path ${worktreePath} already exists`)
  }
  gitStrict(['worktree', 'add', '-b', branch, worktreePath, baseCommit], repositoryPath)
  return { path: worktreePath, branch, baseCommit }
}

/**
 * List the repository-relative files changed in a worktree relative to its base commit.
 * @param workspacePath - worktree root.
 * @param baseCommit - commit the branch started from.
 * @returns the changed paths, including untracked files.
 */
export function changedFiles(workspacePath: string, baseCommit: string): string[] {
  const tracked = git(['diff', '--name-only', baseCommit], workspacePath)
  const untracked = git(['ls-files', '--others', '--exclude-standard'], workspacePath)
  const paths = new Set<string>()
  for (const line of `${tracked.stdout}\n${untracked.stdout}`.split('\n')) {
    const trimmed = line.trim()
    if (trimmed.length > 0) paths.add(trimmed)
  }
  return [...paths].sort()
}

/**
 * Commit every change in one workspace and return the produced commit.
 * @param workspacePath - worktree root.
 * @param message - commit message.
 * @returns the new commit sha, or null when the workspace has no change.
 */
export function commitWorkspace(workspacePath: string, message: string): string | null {
  const changed = git(['status', '--porcelain'], workspacePath)
  if (changed.stdout.trim().length === 0) return null
  gitStrict(['add', '-A'], workspacePath)
  gitStrict(['-c', 'user.name=NC-AG', '-c', 'user.email=ncag@localhost', 'commit', '-m', message], workspacePath)
  return resolveCommit(workspacePath, 'HEAD')
}

/**
 * Submit one workspace as a change set for integration.
 * @param workspacePath - worktree root.
 * @param baseCommit - commit the workspace started from.
 * @param message - commit message.
 * @returns the submitted change.
 */
export function submitChange(workspacePath: string, baseCommit: string, message: string): SubmittedChange {
  const headCommit = commitWorkspace(workspacePath, message)
  if (headCommit === null) {
    throw new Error(`ncag: workspace ${workspacePath} has no change to submit`)
  }
  return { branch: gitStrict(['rev-parse', '--abbrev-ref', 'HEAD'], workspacePath), baseCommit, headCommit, changedFiles: changedFiles(workspacePath, baseCommit) }
}

/**
 * Integrate one submitted change into the base branch.
 * @param repositoryPath - repository root.
 * @param baseBranch - branch the change lands on.
 * @param change - the submitted change.
 * @param validate - check executed on the integrated tree; it returns a failure report or null.
 * @returns the integration outcome.
 */
export function integrateChange(
  repositoryPath: string,
  baseBranch: string,
  change: SubmittedChange,
  validate: (repositoryPath: string) => string | null,
): IntegrationResult {
  const report: string[] = []
  const checkout = git(['checkout', baseBranch], repositoryPath)
  if (!checkout.ok) {
    return { status: 'REJECTED', commit: null, report: [`checkout of ${baseBranch} failed: ${checkout.stderr.trim()}`] }
  }
  const baseChanged = changedFiles(repositoryPath, change.baseCommit)
  if (baseChanged.length > 0) {
    report.push(`base branch moved since submission; overlapping files: ${baseChanged.filter(path => change.changedFiles.includes(path)).length}`)
  }
  // The merge creates a commit, so it needs a committer identity. Supply one
  // explicitly: depending on the host's global git config would make
  // integration fail on a clean machine or a CI container.
  const merge = git(['-c', 'user.name=NC-AG', '-c', 'user.email=ncag@localhost', 'merge', '--no-ff', '--no-edit', change.branch], repositoryPath)
  if (!merge.ok) {
    git(['merge', '--abort'], repositoryPath)
    return {
      status: 'CONFLICT',
      commit: null,
      report: [...report, `textual conflict: ${(merge.stdout + merge.stderr).trim().slice(0, 600)}`],
    }
  }
  const validation = validate(repositoryPath)
  if (validation !== null) {
    report.push(`integration validation failed: ${validation}`)
    const reset = git(['reset', '--hard', 'HEAD~1'], repositoryPath)
    /* v8 ignore next 2 -- the merge commit is always the current HEAD here, so the rollback cannot fail. */
    if (!reset.ok) report.push(`rollback failed: ${reset.stderr.trim()}`)
    else report.push('the merge was rolled back')
    return { status: 'REJECTED', commit: null, report }
  }
  const commit = resolveCommit(repositoryPath, 'HEAD')
  report.push(`merged ${change.branch} into ${baseBranch} as ${commit}`)
  return { status: 'MERGED', commit, report }
}

/**
 * Remove one worktree and delete its branch.
 * @param repositoryPath - repository root.
 * @param worktreePath - worktree to remove.
 * @param branch - branch to delete.
 * @returns whether cleanup succeeded.
 */
export function removeWorkspace(repositoryPath: string, worktreePath: string, branch: string): boolean {
  const removed = git(['worktree', 'remove', '--force', worktreePath], repositoryPath)
  const deleted = git(['branch', '-D', branch], repositoryPath)
  return removed.ok && deleted.ok
}

/**
 * Read one file of the committed tree, used by verification on an integrated repository.
 * @param repositoryPath - repository root.
 * @param path - repository-relative path.
 * @returns the file content, or an empty string when it is missing.
 */
export function readCommittedFile(repositoryPath: string, path: string): string {
  const result = git(['show', `HEAD:${path.replace(/\\/g, '/')}`], repositoryPath)
  return result.ok ? result.stdout : ''
}

/**
 * Repository-relative path of one absolute path.
 * @param repositoryPath - repository root.
 * @param absolutePath - absolute path inside it.
 * @returns the repository-relative path.
 */
export function relativeToRepository(repositoryPath: string, absolutePath: string): string {
  return relative(repositoryPath, absolutePath).split('\\').join('/')
}

/**
 * Ensure a repository exists at one path, creating one when the project requested it.
 * @param repositoryPath - repository root.
 * @param create - whether to initialize a repository when it is missing.
 * @returns whether the path is now a repository.
 */
export function ensureRepository(repositoryPath: string, create: boolean): boolean {
  if (isRepository(repositoryPath)) return true
  if (!create) return false
  mkdirSync(repositoryPath, { recursive: true })
  gitStrict(['init', '-b', 'main'], repositoryPath)
  return isRepository(repositoryPath)
}

/**
 * Absolute path of the worktrees directory a repository owns.
 * @param repositoryPath - repository root.
 * @returns the worktree directory path.
 */
export function worktreeRoot(repositoryPath: string): string {
  return join(repositoryPath, '.ncag-worktrees')
}
