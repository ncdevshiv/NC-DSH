// @vitest-environment jsdom
/**
 * HomeSection component specs: the summary chips, the inbox derivation
 * (blank-filtered, newest-first, capped), and the two action arms (open a
 * row, start a session with busy/error feedback) — plain props stubs, no
 * render machinery beyond the component.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { SessionId, SessionListState, WorkspaceListState } from '@deepseek-ai/dsh-client-runtime/client'
import { HomeSection } from '../src/client/HomeSection.tsx'
import type { HomeSectionProps } from '../src/client/HomeSection.tsx'
// Type-only: pulls this package's LocaleNamespaceMap merge (the 'home' seat).
import type {} from '../src/client/index.ts'
import { en } from '../src/client/locales.ts'

afterEach(cleanup)

const t: HomeSectionProps['t'] = (key, params) => {
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

function sessionState(rows: ReturnType<typeof summary>[], current: SessionId | undefined = undefined): SessionListState {
  return {
    ids: rows.map(row => row.id),
    byId: Object.fromEntries(rows.map(row => [row.id, row])),
    current,
    phase: 'ready',
    subagentsByParent: {},
    jobsBySession: {},
    currentAddress: undefined,
  }
}

function hook<T>(snapshot: T) {
  return function select<S>(selector: (state: T) => S): S { return selector(snapshot) }
}

function mount({ sessions, workspaces = [], startSession = vi.fn(async () => 'n' as SessionId), open = vi.fn() }: {
  sessions: SessionListState
  workspaces?: WorkspaceListState['items']
  startSession?: HomeSectionProps['startSession']
  open?: HomeSectionProps['open']
}) {
  const view = render(<HomeSection
    useSessions={hook(sessions)}
    useWorkspaces={hook({ items: workspaces } as WorkspaceListState)}
    startSession={startSession}
    open={open}
    t={t}
  />)
  return { view, startSession, open }
}

describe('HomeSection', () => {
  it('renders the summary chips from the session and workspace counts', () => {
    mount({
      sessions: sessionState([
        summary('a', { running: true }),
        summary('b'),
        summary('c', { blank: true }),
      ]),
      workspaces: [{ workspaceId: 'w1' }, { workspaceId: 'w2' }] as never,
    })
    expect(screen.getByText('1 running')).toBeTruthy()
    expect(screen.getByText('3 sessions')).toBeTruthy()
    expect(screen.getByText('2 workspaces')).toBeTruthy()
  })

  it('derives the inbox newest-first without blank rows and caps it', () => {
    const rows = Array.from({ length: 35 }, (_, index) => summary(`s${index}`, { updatedAt: index }))
    mount({ sessions: sessionState(rows) })
    const items = screen.getAllByRole('listitem')
    expect(items).toHaveLength(30)
    // Newest first: s34 leads, the blank row never appears even when newest.
    expect(screen.getByText('s34')).toBeTruthy()
    expect(screen.queryByText('s4')).toBeNull()
  })

  it('clicking a row opens that session', () => {
    const open = vi.fn()
    mount({ sessions: sessionState([summary('a'), summary('b')]), open })
    fireEvent.click(screen.getByText('b'))
    expect(open).toHaveBeenCalledWith('b' as SessionId)
  })

  it('renders the empty state when no non-blank session exists', () => {
    mount({ sessions: sessionState([summary('blank-one', { blank: true })]) })
    expect(screen.getByText('No sessions yet. Start one and it lands here.')).toBeTruthy()
  })

  it('shows busy on the quick action while the connect runs', async () => {
    let release!: () => void
    const gate = new Promise<SessionId>((resolve) => { release = () => { resolve('n' as SessionId) } })
    mount({ sessions: sessionState([]), startSession: () => gate })
    fireEvent.click(screen.getByRole('button', { name: 'New Session' }))
    expect(screen.getByRole('button', { name: 'New Session' }).getAttribute('aria-busy')).toBe('true')
    release()
    await vi.waitFor(() => {
      expect(screen.getByRole('button', { name: 'New Session' }).getAttribute('aria-busy')).toBeNull()
    })
  })

  it('renders a rejected start as an alert near the trigger', async () => {
    const { view } = mount({ sessions: sessionState([]), startSession: () => Promise.reject(new Error('busy host')) })
    fireEvent.click(view.getByRole('button', { name: 'New Session' }))
    expect(await view.findByRole('alert')).toBeTruthy()
  })
})
