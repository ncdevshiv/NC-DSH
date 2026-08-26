/**
 * Sidebar shell: column geometry plus the section switcher. Collapse is a
 * slide plus crossfade: content freezes at its expanded width (inline style)
 * and fades out in place while the sliding column (AppFrame grid tracks)
 * clips it — nothing reflows mid-slide. At settle the wide-only content
 * unmounts and the upper controls enter the 56px rail from the same
 * horizontal offset (one icon each, same top-down order) on one fade that
 * ends with the slide. The bottom-pinned settings control only fades. The
 * section switcher owns the region between itself and the foot: wide it is a
 * pill tab strip above New Session, rail it is the icon column; the four
 * section panes stay mounted beneath it (display toggles, like the layout
 * columns) so a section keeps its local state across visits and across
 * collapse. Switching sections animates the incoming pane only — a
 * directional slide + fade retriggered by alternating identical keyframes.
 * The foot holds `sidebar.settings` plus `sidebar.footer.action`; the shell
 * hands them the wide flag.
 *
 * The column also owns whether the scroll regions nested in it draw a
 * scrollbar at all: the shell tracks the pointer and rebinds ui-theme's
 * scrollbar indirection away while it is elsewhere, so a list the user is not
 * pointing at carries no bar.
 */
import { useEffect, useRef, useState } from 'react'
import type { CSSProperties } from 'react'
import clsx from 'clsx'
import {
  FishLogo, IconCodeOutline16, IconHomeOutline16, IconNewChatOutline16,
  IconPanelLeftOutline16, IconTeamOutline16, IconWorkOutline16, Tooltip,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { SidebarRootComponentProps, SidebarSectionKey } from './contract/slots.ts'
import css from './SidebarRoot.module.css'

/** Wide-content unmount delay; matches the 150ms wide-content fade-out. */
const COLLAPSE_SETTLE_MS = 150

/**
 * How long the column's scrollbars stay drawn after the pointer leaves it.
 * The bar is a pointer affordance here, and hiding it on the leave event
 * itself makes it blink out while the pointer is only crossing the column's
 * edge — on the way to the conversation, or around a portalled menu.
 */
const SCROLLBAR_LINGER_MS = 2000

/** The switcher's fixed tab set, in display order; the index distance of a
 *  switch decides its slide direction. */
const SECTIONS: readonly { key: SidebarSectionKey; Icon: (props: { size: number }) => React.ReactNode }[] = [
  { key: 'home', Icon: IconHomeOutline16 },
  { key: 'code', Icon: IconCodeOutline16 },
  { key: 'work', Icon: IconWorkOutline16 },
  { key: 'team', Icon: IconTeamOutline16 },
]

/**
 * Render the sidebar column shell.
 * @param props - composed slot props (runtime share + injected callbacks, contract/slots.ts).
 * @returns the sidebar element tree.
 */
export function SidebarRoot({
  collapsed,
  width,
  startSession,
  toggleSidebar,
  t,
  renderSlot,
}: SidebarRootComponentProps) {
  // Wide content stays mounted while the collapse animates (fading via
  // .collapsed .wide), unmounts at settle, and remounts right away on expand.
  const [settled, setSettled] = useState(collapsed)
  useEffect(() => {
    if (!collapsed) { setSettled(false); return }
    const timer = window.setTimeout(() => { setSettled(true) }, COLLAPSE_SETTLE_MS)
    return () => { window.clearTimeout(timer) }
  }, [collapsed])
  const wide = !collapsed || !settled

  // New Session outcome feedback: the action can legitimately take a while
  // (Host busy with other agents) or fail outright — without this state both
  // looked like a dead button, because the click had no pending/error surface.
  const [startBusy, setStartBusy] = useState(false)
  const [startError, setStartError] = useState<string | null>(null)
  const handleStartSession = (): void => {
    if (startBusy) return
    setStartBusy(true)
    setStartError(null)
    // Promise.resolve keeps sync test doubles (`vi.fn()`) usable.
    void Promise.resolve(startSession()).then(
      () => { setStartBusy(false) },
      (reason: unknown) => {
        setStartBusy(false)
        setStartError(reason instanceof Error ? reason.message : String(reason))
      },
    )
  }

  // Active section: 'code' boots (the workspace browser is the historical
  // region), and every switch bumps seq so the enter animation can retrigger
  // by alternating keyframe names (a same-name animation does not restart).
  const [section, setSection] = useState<{ key: SidebarSectionKey; seq: number; fwd: boolean }>({
    key: 'code', seq: 0, fwd: true,
  })
  const selectSection = (next: SidebarSectionKey): void => {
    setSection((prev) => {
      if (prev.key === next) return prev
      const from = SECTIONS.findIndex(s => s.key === prev.key)
      const to = SECTIONS.findIndex(s => s.key === next)
      return { key: next, seq: prev.seq + 1, fwd: to > from }
    })
  }

  // Freeze the content at its expanded width while it fades out (collapsed
  // && wide): the sliding column then clips it instead of reflowing it. The
  // rail layout (.collapsed styles) only applies once the fade settles.
  const lastWideWidth = useRef(width)
  if (!collapsed) lastWideWidth.current = width

  // Rail-in only crossfades a live collapse: a refresh straight into the
  // collapsed state renders the rail statically (no delay-hidden icons).
  const everWide = useRef(!collapsed)
  if (!collapsed) everWide.current = true

  // Scrollbars in the column follow the pointer (.quietBars rebinds them
  // away): drawn while it is inside, and for SCROLLBAR_LINGER_MS after it
  // leaves. A pointer that returns within that window cancels the pending
  // hide rather than restarting from a hidden bar.
  const column = useRef<HTMLDivElement>(null)
  const [pointerInside, setPointerInside] = useState(false)
  const lingerTimer = useRef<number | undefined>(undefined)
  const armLinger = (): void => {
    if (lingerTimer.current !== undefined) return
    lingerTimer.current = window.setTimeout(() => {
      lingerTimer.current = undefined
      setPointerInside(false)
    }, SCROLLBAR_LINGER_MS)
  }
  const cancelLinger = (): void => {
    window.clearTimeout(lingerTimer.current)
    lingerTimer.current = undefined
  }
  // Leaving is decided by the column's BOX, not by DOM containment, and only
  // while the bars are drawn. ui-settings renders its full-viewport panel as a
  // fixed-position DESCENDANT of this column, so a pointer moved onto that
  // panel — or onto the conversation once it closes — fires no `pointerleave`
  // here, and the bars would stay drawn over a column nobody is pointing at.
  // The element's own leave stays as the one signal geometry cannot give: a
  // pointer that leaves the window emits no further moves.
  useEffect(() => {
    if (!pointerInside) return
    const onMove = (event: PointerEvent): void => {
      const rect = column.current?.getBoundingClientRect()
      /* v8 ignore next -- the listener only exists while the column is mounted and revealed. */
      if (rect === undefined) return
      const inside = event.clientX >= rect.left && event.clientX < rect.right
        && event.clientY >= rect.top && event.clientY < rect.bottom
      if (inside) cancelLinger()
      else armLinger()
    }
    document.addEventListener('pointermove', onMove)
    return () => {
      document.removeEventListener('pointermove', onMove)
      cancelLinger()
    }
  }, [pointerInside])

  return (
    <div
      ref={column}
      className={clsx(
        css.root, !wide && css.collapsed, !wide && everWide.current && css.railIn,
        collapsed && wide && css.fading, !pointerInside && css.quietBars,
      )}
      style={wide ? { width: collapsed ? lastWideWidth.current : width } : undefined}
      onPointerEnter={() => {
        cancelLinger()
        setPointerInside(true)
      }}
      onPointerLeave={() => { armLinger() }}
    >
      {/* Title-bar row: on the desktop shell it is the window's draggable
          left segment (base.css keys the drag region on the attribute), so
          only its two controls opt out. */}
      <div className={css.logoRow} data-dsh-drag-region="">
        {/* Expanded, the brand doubles as a New Session shortcut; the
            collapsed rail's logo is the expand toggle below instead. */}
        {wide && (
          <button
            type="button"
            className={clsx(css.brand, css.wide)}
            aria-label={t('session.new.label')}
            aria-busy={startBusy || undefined}
            onClick={handleStartSession}
          >
            <span className={css.brandIdentity} aria-hidden="true">
              <span className={css.brandMark}>
                {renderSlot('sidebar.brand.mark', { size: 24 }, { fallback: <FishLogo size={24} /> })}
              </span>
              <span className={css.brandName}>
                {renderSlot('sidebar.brand.name', {}, {
                  fallback: (
                    <>
                      <span className={css.fallbackBrandName}>DSH Local Build</span>
                      {process.env.DSH_CLIENT_COMMIT_HASH
                        ? <span className={css.buildRevision}>{process.env.DSH_CLIENT_COMMIT_HASH}</span>
                        : null}
                    </>
                  ),
                })}
              </span>
            </span>
          </button>
        )}
        {/* Rail resting state is the whale mark; hovering swaps in the panel
            icon (the expand affordance, figma sidebar-hover flow). */}
        <Tooltip label={collapsed ? t('toggle.open') : t('toggle.collapse')} delayMs={500}>
          <button
            type="button"
            className={clsx(css.iconButton, css.toggle)}
            aria-label={collapsed ? t('toggle.open') : t('toggle.collapse')}
            onClick={() => { toggleSidebar() }}
          >
            {!wide && (
              <span className={css.railMark} aria-hidden="true">
                {renderSlot('sidebar.brand.mark', { size: 24 }, { fallback: <FishLogo size={24} /> })}
              </span>
            )}
            {/* Rail icons render at 18 (figma rail spec); expanded keeps the glyph-native sizes. */}
            <IconPanelLeftOutline16 className={css.panelIcon} size={wide ? 16 : 18} />
          </button>
        </Tooltip>
      </div>

      {/* Section switcher: pill tabs wide, the icon rail collapsed. A rail
          click also expands the column — the section panes are hidden while
          collapsed, so selecting without expanding would show nothing. */}
      {wide ? (
        <div className={css.sectionTabs} role="tablist">
          {SECTIONS.map(({ key, Icon }) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={section.key === key}
              className={clsx(css.sectionTab, section.key === key && css.sectionTabActive)}
              onClick={() => { selectSection(key) }}
            >
              <Icon size={14} />
              <span className={css.sectionTabLabel}>{t(`section.${key}`)}</span>
            </button>
          ))}
        </div>
      ) : (
        <div className={css.sectionRail}>
          {SECTIONS.map(({ key, Icon }) => (
            <Tooltip key={key} label={t(`section.${key}`)} delayMs={500}>
              <button
                type="button"
                className={clsx(css.iconButton, css.sectionRailTab, section.key === key && css.sectionRailActive)}
                aria-label={t(`section.${key}`)}
                onClick={() => {
                  selectSection(key)
                  if (collapsed) toggleSidebar()
                }}
              >
                <Icon size={18} />
              </button>
            </Tooltip>
          ))}
        </div>
      )}

      {/* Expanded, the button carries its own label — tooltip only on the rail. */}
      <Tooltip label={t('session.new.label')} delayMs={500} disabled={wide}>
        <button
          type="button"
          className={css.newSession}
          aria-label={t('session.new.label')}
          aria-busy={startBusy || undefined}
          onClick={handleStartSession}
        >
          <IconNewChatOutline16 size={wide ? 14 : 18} />
          {wide && <span className={clsx(css.newSessionLabel, css.wide)}>{startBusy ? t('session.new.connecting') : t('session.new')}</span>}
        </button>
      </Tooltip>
      {/* Connect failures reject after the runtime logged them; without this
          line the failure was indistinguishable from a dead button. */}
      {startError !== null && (
        <p className={css.connectError} role="alert">{t('session.new.failed', { message: startError })}</p>
      )}

      {/* The section panes fill the column between the switcher controls and
          the foot in both states. They stay mounted (display-toggled) so a
          section keeps its local state across visits and across a collapse;
          the active pane carries the one-shot enter animation, retriggered
          per switch by alternating identical keyframe names and aimed by the
          --section-slide-from custom property. */}
      <div className={css.regionArea}>
        {SECTIONS.map(({ key }) => (
          <div
            key={key}
            className={clsx(
              css.sectionPane,
              key === section.key ? css.sectionPaneActive : css.sectionPaneHidden,
              key === section.key && section.seq > 0
                ? (section.seq % 2 === 1 ? css.sectionPaneEnterA : css.sectionPaneEnterB)
                : undefined,
            )}
            style={key === section.key && section.seq > 0
              ? ({ '--section-slide-from': section.fwd ? '16px' : '-16px' }) as CSSProperties
              : undefined}
            aria-hidden={key !== section.key || undefined}
          >
            {renderSlot('sidebar.section', {}, { entryKey: key })}
          </div>
        ))}
      </div>

      {/* Footer actions stack above Settings in both sidebar widths. */}
      <div className={css.footArea}>
        <div className={css.footerActions}>
          {renderSlot('sidebar.footer.action', { wide })}
        </div>
        <div className={css.settingsArea}>
          {renderSlot('sidebar.settings', { wide })}
        </div>
      </div>
    </div>
  )
}
