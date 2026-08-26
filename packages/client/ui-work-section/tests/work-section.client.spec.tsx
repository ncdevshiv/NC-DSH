// @vitest-environment jsdom
/**
 * WorkSection component specs: the board partitioning (needs-you leads,
 * running excludes attention rows, goals render only materialized
 * projections), the goal phase chips with blocked reasons, and row opening.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { SessionId, SessionListState, SessionSummary } from '@deepseek-ai/dsh-client-runtime/client'
import { WorkSection } from '../src/client/WorkSection.tsx'
import type { WorkSectionProps } from '../src/client/WorkSection.tsx'
// Type-only: pulls this package's LocaleNamespaceMap merge (the 'work' seat).
import type {} from '../src/client/index.ts'
import type { GoalProjection } from '@deepseek-ai/dsh-goal/client'
import { en } from '../src/client/locales.ts'

afterEach(cleanup)

const t: WorkSectionProps['t'] = (key, params) => {
  let value = (en as Record<string, string>)[key] ?? key
  if (params !== undefined) {
    for (const [name, replacement] of Object.entries(params)) value = value.replaceAll(`{${name}}`, String(replacement))
  }
  return value
}

function summary(id: string, overrides: Partial<SessionSummary> = {}) {
  return {
    id: id as SessionId,
    displayTitle: id,
    running: false,
    blank: false,
    updatedAt: 1_000,
    ...overrides,
  }
}

function sessionState(rows: SessionSummary[]): SessionListState {
  return {
    ids: rows.map(row => row.id),
    byId: Object.fromEntries(rows.map(row => [row.id, row])),
    current: undefined,
    phase: 'ready',
    subagentsByParent: {},
    jobsBySession: {},
    currentAddress: undefined,
  }
}

function hook<T>(snapshot: T) {
  return function select<S>(selector: (state: T) => S): S { return selector(snapshot) }
}

// The section reads no workspace data, but the global seat still rides the
// props share; stub it as never-called.
const neverHook = (() => { throw new Error('section must not read this hook') }) as never

function mount(sessions: SessionListState, open = vi.fn()) {
  return { view: render(<WorkSection useSessions={hook(sessions)} useWorkspaces={neverHook} open={open} t={t} />), open }
}

const goalProjection = (phase: 'active' | 'paused' | 'blocked', objective = 'ship the release'): GoalProjection => ({
  goal: {
    id: 'goal-1' as GoalProjection['goal']['id'],
    revision: 1,
    objective,
    phase,
    maxGoalRounds: 30,
    ...(phase === 'blocked' ? { blockedReason: { code: 'stalled', message: 'No progress in 3 rounds' } } : {}),
  },
  roundsStarted: 2,
  createdAt: 900,
  updatedAt: 1_500,
})

describe('WorkSection', () => {
  it('partitions the board: attention leads, running excludes attention rows', () => {
    const lists = mount(sessionState([
      summary('idle-one'),
      summary('busy-one', { running: true }),
      summary('blocked-one', { running: true, pendingInteraction: 'approval' }),
      summary('asked-one', { pendingInteraction: 'question' }),
    ])).view
    const lists_all = lists.getAllByRole('list')
    // Two non-empty lists: needs-you (2 rows) then running (1 row).
    expect(lists_all[0]!.textContent).toContain('blocked-one')
    expect(lists_all[0]!.textContent).toContain('asked-one')
    expect(lists_all[1]!.textContent).toContain('busy-one')
    expect(lists_all[1]!.textContent).not.toContain('blocked-one')
    // The idle row appears nowhere.
    expect(lists.queryByText('idle-one')).toBeNull()
  })

  it('renders the empty states when nothing waits and nothing runs', () => {
    mount(sessionState([summary('idle-one')]))
    expect(screen.getByText('Nothing is waiting on you.')).toBeTruthy()
    expect(screen.getByText('No sessions are running right now.')).toBeTruthy()
  })

  it('renders materialized goal projections with phase chips and blocked reasons', () => {
    mount(sessionState([
      summary('goal-active', {
        running: true,
        projectionValues: { goal: goalProjection('active') },
      }),
      summary('goal-blocked', {
        projectionValues: { goal: goalProjection('blocked', 'fix the flake') },
      }),
      summary('no-goal'),
    ]))
    expect(screen.getByText('ship the release')).toBeTruthy()
    expect(screen.getByText('fix the flake')).toBeTruthy()
    expect(screen.getByText('Blocked')).toBeTruthy()
    expect(screen.getByText('No progress in 3 rounds')).toBeTruthy()
    expect(screen.getByText('Active')).toBeTruthy()
    // Sessions without a materialized projection never render a goal row.
    expect(screen.queryByText('no-goal')).toBeNull()
  })

  it('clicking a row opens that session', () => {
    const open = vi.fn()
    const { view } = mount(sessionState([summary('busy-one', { running: true })]), open)
    fireEvent.click(view.getByText('busy-one'))
    expect(open).toHaveBeenCalledWith('busy-one' as SessionId)
  })
})
