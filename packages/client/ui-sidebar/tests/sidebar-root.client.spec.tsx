// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import type {
  SidebarFooterActionOwnerProps, SidebarRootComponentProps, SidebarSettingsOwnerProps,
} from '../src/client/contract/slots.ts'
import { SidebarRoot } from '../src/client/SidebarRoot.tsx'
import { en } from '../src/client/locales.ts'

// English-dictionary translate stub: the shell renders the same copy the
// assertions below query by accessible name.
const t: SidebarRootComponentProps['t'] = key => (en as Record<string, string>)[key] ?? key

afterEach(() => {
  cleanup()
  vi.unstubAllEnvs()
  vi.useRealTimers()
})

// The shell never reads the global hooks itself, but they ride the standard
// props share; stub them as never-called functions.
const neverHook = (() => { throw new Error('shell must not read global hooks') }) as never

function mountShell({ collapsed = false, width = 300 }: { collapsed?: boolean; width?: number } = {}) {
  const startSession = vi.fn()
  const toggleSidebar = vi.fn()
  let settingsOwner: SidebarSettingsOwnerProps | undefined
  let footerActionOwner: SidebarFooterActionOwnerProps | undefined
  const brandMark = <span data-testid="custom-brand-mark">M</span>
  const brandName = <span data-testid="custom-brand-name">Custom Brand</span>
  let current = { collapsed, width }
  const root = () => (
    <SidebarRoot
      collapsed={current.collapsed} width={current.width}
      useSessions={neverHook} useWorkspaces={neverHook}
      startSession={startSession} toggleSidebar={toggleSidebar} t={t}
      renderSlot={((
        key: string,
        owner: SidebarFooterActionOwnerProps | SidebarSettingsOwnerProps,
        options?: { entryKey?: string },
      ) => {
        if (key === 'sidebar.brand.mark') return brandMark
        if (key === 'sidebar.brand.name') return brandName
        if (key === 'sidebar.settings') {
          settingsOwner = owner
          return <div data-testid="settings-seat" data-wide={owner.wide} />
        }
        if (key === 'sidebar.footer.action') {
          footerActionOwner = owner
          return <div data-testid="footer-action-seat" data-wide={owner.wide} />
        }
        // sidebar.section dispatches once per key; each stub node sits inside
        // its pane wrapper, so tests read the wrapper's aria-hidden state.
        return <div data-testid={`section-${options?.entryKey}`} />
      }) as SidebarRootComponentProps['renderSlot']}
    />
  )
  const view = render(root())
  return {
    startSession,
    toggleSidebar,
    settingsOwner: () => {
      if (settingsOwner === undefined) throw new Error('settings owner not rendered')
      return settingsOwner
    },
    footerActionOwner: () => {
      if (footerActionOwner === undefined) throw new Error('footer action owner not rendered')
      return footerActionOwner
    },
    rerender(next: Partial<typeof current>) {
      current = { ...current, ...next }
      view.rerender(root())
    },
    container: view.container,
  }
}

/** The pane wrapper around one section stub, for aria-hidden assertions. */
function paneOf(testId: string): HTMLElement {
  const node = screen.getByTestId(testId)
  const pane = node.parentElement
  if (pane === null) throw new Error(`pane wrapper missing for ${testId}`)
  return pane
}

describe('SidebarRoot shell', () => {
  it('routes New Session (capsule + wordmark) and the column toggle', async () => {
    let release!: () => void
    const gate = new Promise<void>((resolve) => { release = resolve })
    const b = mountShell()
    b.startSession.mockImplementation(() => gate)
    expect(screen.getByTestId('custom-brand-mark')).toBeTruthy()
    expect(screen.getByTestId('custom-brand-name')).toBeTruthy()
    // Expanded, both the wordmark and the capsule start a session.
    const starters = screen.getAllByRole('button', { name: 'New session' })
    expect(starters).toHaveLength(2)
    // One connect at a time: New Session mints fresh sessions, so concurrent
    // clicks would create duplicates — every trigger shares the busy guard
    // and the capsule announces the pending state.
    for (const button of starters) fireEvent.click(button)
    expect(b.startSession).toHaveBeenCalledOnce()
    expect(screen.getByText('Creating…')).toBeTruthy()
    release()
    await act(async () => {})
    expect(screen.queryByText('Creating…')).toBeNull()
    fireEvent.click(starters[1]!)
    expect(b.startSession).toHaveBeenCalledTimes(2)
    fireEvent.click(screen.getByRole('button', { name: 'Collapse sidebar' }))
    expect(b.toggleSidebar).toHaveBeenCalledOnce()
  })

  it('renders a rejected connect as an alert instead of dying silently', async () => {
    const b = mountShell()
    b.startSession.mockImplementation(() => Promise.reject(new Error('session-create-timeout')))
    fireEvent.click(screen.getAllByRole('button', { name: 'New session' })[0]!)
    const alert = await screen.findByRole('alert')
    // The naive t stub in this file does not interpolate; assert the prefix.
    expect(alert.textContent?.startsWith('New session failed:')).toBe(true)
    // Recovery: a later successful click clears the alert.
    b.startSession.mockImplementation(() => Promise.resolve('fk-ok' as never))
    fireEvent.click(screen.getAllByRole('button', { name: 'New session' })[0]!)
    await act(async () => {})
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('renders generic brand fallbacks when no package fills the slots', () => {
    vi.stubEnv('DSH_CLIENT_COMMIT_HASH', '0123456')
    const { container } = render(<SidebarRoot
      collapsed={false} width={300}
      useSessions={neverHook} useWorkspaces={neverHook}
      startSession={vi.fn()} toggleSidebar={vi.fn()} t={t}
      renderSlot={((_key: string, _owner: unknown, options?: { fallback?: ReactNode }) =>
        options?.fallback ?? null) as SidebarRootComponentProps['renderSlot']}
    />)

    expect(screen.getByText('DSH Local Build')).toBeTruthy()
    expect(screen.getByText('0123456')).toBeTruthy()
    // The fish mark fallback renders in both the brand row and the toggle.
    expect(container.querySelector('svg')).not.toBeNull()
  })

  it('boots on Code and switches the active pane through the tab strip', () => {
    const b = mountShell()
    const tabs = screen.getAllByRole('tab')
    expect(tabs.map(tab => tab.textContent)).toEqual(['Home', 'Code', 'Work', 'Team'])
    // Boot state: Code is the active tab and the only revealed pane.
    expect(tabs[1]!.getAttribute('aria-selected')).toBe('true')
    expect(paneOf('section-code').getAttribute('aria-hidden')).toBeNull()
    expect(paneOf('section-home').getAttribute('aria-hidden')).toBe('true')
    // Switch forward: Work becomes active, Code hides.
    fireEvent.click(screen.getByRole('tab', { name: 'Work' }))
    expect(screen.getByRole('tab', { name: 'Work' }).getAttribute('aria-selected')).toBe('true')
    expect(paneOf('section-work').getAttribute('aria-hidden')).toBeNull()
    expect(paneOf('section-code').getAttribute('aria-hidden')).toBe('true')
    // Switching to the active tab is a no-op.
    fireEvent.click(screen.getByRole('tab', { name: 'Work' }))
    expect(paneOf('section-work').getAttribute('aria-hidden')).toBeNull()
    // The settings and footer seats still ride the wide flag.
    expect(b.settingsOwner().wide).toBe(true)
    expect(b.footerActionOwner().wide).toBe(true)
  })

  it('renders the section icon rail when collapsed and expands on a pick', () => {
    vi.useFakeTimers()
    const b = mountShell()
    b.rerender({ collapsed: true })
    // Wide content survives the crossfade window, then settles into the rail.
    vi.advanceTimersByTime(200)
    b.rerender({})
    // The tab strip is gone; the rail carries one icon button per section.
    expect(screen.queryByRole('tab')).toBeNull()
    for (const name of ['Home', 'Code', 'Work', 'Team']) {
      expect(screen.getByRole('button', { name })).toBeTruthy()
    }
    // The rail keeps Code marked active without rendering the section area.
    expect(screen.getByRole('button', { name: 'Code' }).className).toContain('sectionRailActive')
    // Picking a section selects it AND expands the column (the panes are
    // hidden while collapsed, so selecting without expanding shows nothing).
    fireEvent.click(screen.getByRole('button', { name: 'Team' }))
    expect(b.toggleSidebar).toHaveBeenCalledOnce()
    b.rerender({ collapsed: false })
    expect(screen.getByRole('tab', { name: 'Team' }).getAttribute('aria-selected')).toBe('true')
    expect(paneOf('section-team').getAttribute('aria-hidden')).toBeNull()
  })

  it('renders statically collapsed on a cold start (no crossfade classes)', () => {
    mountShell({ collapsed: true })
    expect(screen.getByRole('button', { name: 'Open sidebar' })).toBeTruthy()
    expect(screen.queryByRole('tab')).toBeNull()
  })
})
