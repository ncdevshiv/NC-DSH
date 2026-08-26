// @vitest-environment jsdom
/**
 * TeamSection component specs: the members derivation (subagent-origin rows,
 * running first), the roster store rendering (ready / error / broken cards),
 * and the two action arms (open a member, start-with with busy/error).
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { SessionId, SessionListState } from '@deepseek-ai/dsh-client-runtime/client'
import { TeamSection } from '../src/client/TeamSection.tsx'
import type { TeamRosterState, TeamSectionProps } from '../src/client/TeamSection.tsx'
// Type-only: pulls this package's LocaleNamespaceMap merge (the 'team' seat).
import type {} from '../src/client/index.ts'
import { en } from '../src/client/locales.ts'

afterEach(cleanup)

const t: TeamSectionProps['t'] = (key, params) => {
  let value = (en as Record<string, string>)[key] ?? key
  if (params !== undefined) {
    for (const [name, replacement] of Object.entries(params)) value = value.replaceAll(`{${name}}`, String(replacement))
  }
  return value
}

function summary(id: string, overrides: Partial<SessionListState['byId'][SessionId]> = {}) {
  return {
    id: id as SessionId,
    displayTitle: id,
    running: false,
    blank: false,
    updatedAt: 1_000,
    ...overrides,
  }
}

function sessionState(rows: ReturnType<typeof summary>[]): SessionListState {
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

function rosterHook(state: TeamRosterState) {
  // The bound use<Name> seat selects over the observable's snapshot; the
  // stub hands the selector the state directly.
  return hook(state)
}

// The section reads no workspace data, but the global seat still rides the
// props share; stub it as never-called.
const neverHook = (() => { throw new Error('section must not read this hook') }) as never

function mount({ sessions, roster, openMember = vi.fn(), startWith = vi.fn(async () => {}) }: {
  sessions: SessionListState
  roster: TeamRosterState
  openMember?: TeamSectionProps['openMember']
  startWith?: TeamSectionProps['startWith']
}) {
  return render(<TeamSection
    useSessions={hook(sessions)}
    useWorkspaces={neverHook}
    useRoster={rosterHook(roster)}
    openMember={openMember}
    startWith={startWith}
    t={t}
  />)
}

describe('TeamSection', () => {
  it('renders only subagent-origin rows as members, running first', () => {
    mount({
      sessions: sessionState([
        summary('root-a'),
        summary('child-1', { origin: 'subagent', running: false, updatedAt: 500 }),
        summary('child-2', { origin: 'subagent', running: true, updatedAt: 100 }),
      ]),
      roster: { status: 'ready', presets: [] },
    })
    const items = screen.getAllByRole('listitem')
    expect(items).toHaveLength(2)
    // Running member leads regardless of recency.
    expect(items[0]!.textContent).toContain('child-2')
    expect(screen.queryByText('root-a')).toBeNull()
    // The running count chip rides the section title.
    expect(screen.getByText('1')).toBeTruthy()
  })

  it('clicking a member opens it through the injected verb', () => {
    const openMember = vi.fn()
    mount({
      sessions: sessionState([summary('child-1', { origin: 'subagent' })]),
      roster: { status: 'ready', presets: [] },
      openMember,
    })
    fireEvent.click(screen.getByText('child-1'))
    expect(openMember).toHaveBeenCalledWith('child-1' as SessionId)
  })

  it('renders the members empty state when no subagent exists', () => {
    mount({
      sessions: sessionState([summary('root-a')]),
      roster: { status: 'ready', presets: [] },
    })
    expect(screen.getByText('No agents are running. Delegate work to a subagent and it appears here.')).toBeTruthy()
  })

  it('renders roster cards with trust badges and a start trigger each', () => {
    mount({
      sessions: sessionState([]),
      roster: {
        status: 'ready',
        presets: [
          { id: 'coder', trust: 'system', name: 'Coder', description: 'Writes code.' },
          { id: 'reviewer', trust: 'user', name: 'Reviewer' },
        ],
      },
    })
    expect(screen.getByText('Coder')).toBeTruthy()
    expect(screen.getByText('Writes code.')).toBeTruthy()
    expect(screen.getByText('System')).toBeTruthy()
    expect(screen.getByText('User')).toBeTruthy()
    expect(screen.getAllByRole('button', { name: 'Start session' })).toHaveLength(2)
  })

  it('a broken preset shows the reason instead of a start trigger', () => {
    mount({
      sessions: sessionState([]),
      roster: {
        status: 'ready',
        presets: [{ id: 'ghost', trust: 'user', broken: 'composition missing' }],
      },
    })
    expect(screen.getByText('Unavailable: composition missing')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Start session' })).toBeNull()
  })

  it('startWith drives the trigger busy state through the connect', async () => {
    let release!: () => void
    const gate = new Promise<void>((resolve) => { release = resolve })
    mount({
      sessions: sessionState([]),
      roster: { status: 'ready', presets: [{ id: 'coder', trust: 'system' }] },
      startWith: () => gate,
    })
    fireEvent.click(screen.getByRole('button', { name: 'Start session' }))
    expect(screen.getByRole('button', { name: 'Start session' }).getAttribute('aria-busy')).toBe('true')
    release()
    await vi.waitFor(() => {
      expect(screen.getByRole('button', { name: 'Start session' }).getAttribute('aria-busy')).toBeNull()
    })
  })

  it('renders a rejected start as an alert', async () => {
    const view = mount({
      sessions: sessionState([]),
      roster: { status: 'ready', presets: [{ id: 'coder', trust: 'system' }] },
      startWith: () => Promise.reject(new Error('preset refused')),
    })
    fireEvent.click(view.getByRole('button', { name: 'Start session' }))
    expect(await view.findByRole('alert')).toBeTruthy()
  })

  it('renders the roster error state with the message', () => {
    mount({
      sessions: sessionState([]),
      roster: { status: 'error', error: 'wire down' },
    })
    expect(screen.getByText('Roster unavailable: wire down')).toBeTruthy()
  })
})
