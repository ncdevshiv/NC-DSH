import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { resolveGitExecutable } from './harness.ts'
import {
  changedFiles, commitWorkspace, createWorkspace, ensureRepository, git, gitExecutable, gitStrict,
  integrateChange, isRepository, readCommittedFile, relativeToRepository, removeWorkspace, resolveCommit,
  submitChange, worktreeRoot,
} from '../src/git.ts'

let root: string
let repository: string

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'ncag-git-'))
  repository = join(root, 'repo')
  mkdirSync(repository, { recursive: true })
  gitStrict(['init', '-b', 'main'], repository)
  writeFileSync(join(repository, 'a.txt'), 'base\n')
  commit(repository, 'initial')
})

afterEach(() => {
  rmSync(root, { recursive: true, force: true })
})

function commit(cwd: string, message: string): string {
  gitStrict(['add', '-A'], cwd)
  gitStrict(['-c', 'user.name=NC-AG', '-c', 'user.email=ncag@localhost', 'commit', '-m', message], cwd)
  return resolveCommit(cwd, 'HEAD')
}

// Git worktree creation and merging are process-heavy on Windows and can take
// several seconds each; the suite times out at the default per-test budget.
describe('ncag git workspace operations', { timeout: 30_000 }, () => {
  it('resolves the executable and reports command outcomes', () => {
    expect(gitExecutable()).toBe(resolveGitExecutable())
    expect(git(['rev-parse', 'HEAD'], repository).ok).toBe(true)
    const failure = git(['rev-parse', 'no-such-ref'], repository)
    expect(failure.ok).toBe(false)
    expect(failure.exitCode).not.toBe(0)
    expect(() => gitStrict(['rev-parse', 'no-such-ref'], repository)).toThrow(/git rev-parse/)
    // A failure with an empty stderr falls back to stdout for its report.
    expect(() => gitStrict(['config', '--get', 'ncag.no.such.key'], repository)).toThrow(/git config/)
    expect(resolveCommit(repository, 'HEAD')).toMatch(/^[0-9a-f]{40}$/)
    expect(isRepository(repository)).toBe(true)
  })

  it('creates an isolated worktree, refuses a missing repository and an occupied path', () => {
    const base = resolveCommit(repository, 'main')
    const workspace = createWorkspace(repository, join(root, 'wt'), 'ncag/one', 'main')
    expect(workspace.baseCommit).toBe(base)
    expect(isRepository(workspace.path)).toBe(true)
    expect(() => createWorkspace(repository, join(root, 'wt'), 'ncag/two', 'main')).toThrow(/already exists/)
    expect(() => createWorkspace(join(root, 'missing'), join(root, 'wt2'), 'ncag/three', 'main')).toThrow(/does not exist/)
  })

  it('lists tracked and untracked changes and commits them', () => {
    const workspace = createWorkspace(repository, join(root, 'wt'), 'ncag/one', 'main')
    writeFileSync(join(workspace.path, 'a.txt'), 'changed\n')
    writeFileSync(join(workspace.path, 'new.txt'), 'new\n')
    expect(changedFiles(workspace.path, workspace.baseCommit)).toEqual(['a.txt', 'new.txt'])
    const submission = submitChange(workspace.path, workspace.baseCommit, 'submit')
    expect(submission.changedFiles).toEqual(['a.txt', 'new.txt'])
    expect(commitWorkspace(workspace.path, 'again')).toBeNull()
    const empty = createWorkspace(repository, join(root, 'wt-empty'), 'ncag/empty', 'main')
    expect(() => submitChange(empty.path, empty.baseCommit, 'nothing')).toThrow(/has no change to submit/)
  })

  it('integrates a clean change, reports a moved base and rolls back a failed validation', () => {
    const workspace = createWorkspace(repository, join(root, 'wt'), 'ncag/one', 'main')
    writeFileSync(join(workspace.path, 'b.txt'), 'b\n')
    const submission = submitChange(workspace.path, workspace.baseCommit, 'add b')
    // Move the base branch after the workspace was created.
    writeFileSync(join(repository, 'c.txt'), 'c\n')
    commit(repository, 'advance base')
    const moved = integrateChange(repository, 'main', submission, () => null)
    expect(moved.status).toBe('MERGED')
    expect(moved.report.some(line => line.includes('base branch moved'))).toBe(true)
    expect(readCommittedFile(repository, 'b.txt')).toContain('b')
    expect(readCommittedFile(repository, 'missing.txt')).toBe('')

    const second = createWorkspace(repository, join(root, 'wt2'), 'ncag/two', 'main')
    writeFileSync(join(second.path, 'd.txt'), 'd\n')
    const secondSubmission = submitChange(second.path, second.baseCommit, 'add d')
    const rejected = integrateChange(repository, 'main', secondSubmission, () => 'checks failed')
    expect(rejected.status).toBe('REJECTED')
    expect(rejected.commit).toBeNull()
    expect(rejected.report.some(line => line.includes('rolled back'))).toBe(true)
  })

  it('reports a checkout failure and a textual merge conflict', () => {
    const base = resolveCommit(repository, 'main')
    const checkoutFailure = integrateChange(repository, 'no-such-branch', { branch: 'main', baseCommit: base, headCommit: base, changedFiles: [] }, () => null)
    expect(checkoutFailure.status).toBe('REJECTED')
    expect(checkoutFailure.report[0]).toContain('checkout of no-such-branch failed')

    const workspace = createWorkspace(repository, join(root, 'wt'), 'ncag/one', 'main')
    writeFileSync(join(workspace.path, 'a.txt'), 'workspace\n')
    const submission = submitChange(workspace.path, workspace.baseCommit, 'workspace edit')
    writeFileSync(join(repository, 'a.txt'), 'base edit\n')
    commit(repository, 'conflicting edit')
    const conflict = integrateChange(repository, 'main', submission, () => null)
    expect(conflict.status).toBe('CONFLICT')
    expect(conflict.report.some(line => line.includes('textual conflict'))).toBe(true)
    expect(readCommittedFile(repository, 'a.txt')).toContain('base edit')
  })

  it('removes a worktree, derives relative paths and ensures repositories', () => {
    const workspace = createWorkspace(repository, join(root, 'wt'), 'ncag/one', 'main')
    expect(worktreeRoot(repository)).toBe(join(repository, '.ncag-worktrees'))
    expect(relativeToRepository(repository, join(repository, 'a.txt'))).toBe('a.txt')
    expect(relativeToRepository(repository, join(repository, 'nested', 'b.txt'))).toBe('nested/b.txt')
    expect(removeWorkspace(repository, workspace.path, workspace.branch)).toBe(true)
    // A second removal fails because the worktree is gone.
    expect(removeWorkspace(repository, workspace.path, 'ncag/missing')).toBe(false)

    expect(ensureRepository(repository, false)).toBe(true)
    expect(ensureRepository(join(root, 'nope'), false)).toBe(false)
    const created = join(root, 'created')
    expect(ensureRepository(created, true)).toBe(true)
    expect(isRepository(created)).toBe(true)
    // The repository already exists, so creation is a no-op.
    expect(ensureRepository(created, true)).toBe(true)
  })

  it('keeps the committed tree readable only through git', () => {
    const head = resolveCommit(repository, 'HEAD')
    expect(head).not.toBe('')
    expect(git(['status', '--porcelain'], repository).stdout.trim()).toBe('')
    expect(existsSync(join(repository, 'a.txt'))).toBe(true)
  })
})
