/**
 * Sidebar-foot notifications entry: one bell trigger plus the fixed dropdown
 * panel it opens above the footer (the sidebar clips overflow, so the panel
 * hugs the trigger through a measured fixed offset instead of document flow).
 * The panel holds two sections — the system-notice list and the AI SDK update
 * card — and drives every fact through the injected controller; the component
 * owns only popover visibility and the sampled clock for relative times.
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent } from 'react'
import { Button, IconCloseFill14, useDismissOnOutsidePointer } from '@deepseek-ai/dsh-client-ui-primitives'
import type { IconProps } from '@deepseek-ai/dsh-client-ui-primitives'
import type { InjectFace, PropsLocale, PropsRuntime } from '@deepseek-ai/dsh-client-ui-slots'
import type { NotificationView, NotificationsController } from './store.ts'
import { NS } from './locales.ts'
import type { TranslateNS } from '@deepseek-ai/dsh-client-ui-slots'
// Type-only: pulls ui-sidebar's SlotMap merge (the 'sidebar.footer.action'
// list entry) into this program, so PropsRuntime<'sidebar.footer.action'>
// resolves.
import type {} from '@deepseek-ai/dsh-client-ui-sidebar/client'
import css from './NotificationsBell.module.css'

/** Registration-side dependencies of {@linkcode NotificationsBell}. */
export interface NotificationsInjected {
  /** The panel's snapshot source, bound by the renderer as `useSnapshot`. */
  hooks: {
    snapshot: NotificationsController['store']
  }
  /** Notifications controller: reads and the complete write surface. */
  controller: NotificationsController
}

/** Full composed props of the sidebar-footer notifications entry. */
export type NotificationsBellProps =
  PropsRuntime<'sidebar.footer.action'> & InjectFace<NotificationsInjected> & PropsLocale<typeof NS>

/** Update-status poll cadence while the entry is mounted. */
const STATUS_POLL_INTERVAL_MS = 60_000

/**
 * Bell glyph for the trigger. The shared icon table carries no bell yet, so
 * this package owns its own outline-stroke glyph on the same 16 grid.
 */
function IconBellOutline16({ size = 16, className }: IconProps) {
  return (
    <svg width={size} height={size} className={className} viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path
        d="M12 5.333A4 4 0 0 0 4 5.333c0 4.667-2 6-2 6h12s-2-1.333-2-6"
        stroke="currentColor"
        strokeWidth="1.333"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M9.153 14a1.333 1.333 0 0 1-2.307 0"
        stroke="currentColor"
        strokeWidth="1.333"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/**
 * Age of one notice as a coarse human figure. Beyond thirty days the creation
 * date itself reads better than a growing week/month vocabulary no producer
 * currently reaches; an unparsable stamp renders as nothing rather than as a
 * wrong figure.
 */
function relativeTime(iso: string, nowMs: number, t: TranslateNS<typeof NS>): string {
  const then = Date.parse(iso)
  if (Number.isNaN(then)) return ''
  const elapsed = Math.max(0, nowMs - then)
  if (elapsed < 60_000) return t('time.justnow')
  if (elapsed < 3_600_000) return t('time.minutes', { minutes: Math.floor(elapsed / 60_000) })
  if (elapsed < 86_400_000) return t('time.hours', { hours: Math.floor(elapsed / 3_600_000) })
  if (elapsed < 30 * 86_400_000) return t('time.days', { days: Math.floor(elapsed / 86_400_000) })
  return iso.slice(0, 10)
}

/**
 * Render the notifications bell and its dropdown panel.
 * @param props - sidebar owner state, the injected controller face, and the namespace translator.
 * @returns the trigger element tree; the panel renders only while open.
 */
export function NotificationsBell({
  wide,
  useSnapshot,
  controller,
  t,
}: NotificationsBellProps) {
  const snapshot = useSnapshot(state => state)
  const [open, setOpen] = useState(false)
  const [now, setNow] = useState(() => Date.now())
  const rootRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const [anchor, setAnchor] = useState<{ left: number; bottom: number }>()

  // Status freshness rides the mount lifetime: one immediate sync, a poll
  // every interval, and a refetch when the window regains focus.
  useEffect(() => {
    void controller.refreshStatus()
    const timer = window.setInterval(() => { void controller.refreshStatus() }, STATUS_POLL_INTERVAL_MS)
    const refetch = (): void => { void controller.refreshStatus() }
    window.addEventListener('focus', refetch)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener('focus', refetch)
    }
  }, [controller])

  // Opening pulls the notice list fresh (and again after any mutation, inside
  // the controller); the clock sample rides the same commit so ages never
  // render against a stale mount-time value.
  useEffect(() => {
    if (!open) return
    setNow(Date.now())
    void controller.refreshNotices()
  }, [controller, open])

  useLayoutEffect(() => {
    if (!open) return
    const place = (): void => {
      const rect = rootRef.current?.getBoundingClientRect()
      /* v8 ignore next 1 -- the ref attaches in the same commit as this effect, so it cannot be null here. */
      if (rect !== undefined) setAnchor({ left: rect.left, bottom: window.innerHeight - rect.top + 8 })
    }
    place()
    window.addEventListener('resize', place)
    return () => { window.removeEventListener('resize', place) }
  }, [open])

  useDismissOnOutsidePointer(rootRef, open, setOpen)

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    if (event.key !== 'Escape' || !open) return
    event.preventDefault()
    setOpen(false)
    triggerRef.current?.focus()
  }

  const updates = snapshot.updates
  const latest = updates?.latest ?? null
  const installedTag = updates?.installed?.tag ?? null
  const offerUpdate = updates !== null && updates.updateAvailable && !updates.ignoredLatest && latest !== null
  const successTag = snapshot.installedNow
  const busy = snapshot.installing
  // Each domain reports its own inline line; a host-reported lastError rides
  // beside the local status failures it describes.
  const sdkErrorLine = snapshot.sdkError ?? updates?.lastError ?? null
  const noticesErrorLine = snapshot.noticesError

  const liveNotices = useMemo(
    () => snapshot.notices.filter(notice => !notice.dismissed),
    [snapshot.notices],
  )
  const unreadCount = useMemo(
    () => snapshot.notices.filter(notice => !notice.read && !notice.dismissed).length,
    [snapshot.notices],
  )
  const updateDot = updates !== null && updates.updateAvailable && !updates.ignoredLatest && unreadCount === 0

  const toggle = (): void => {
    if (!open) setNow(Date.now())
    setOpen(current => !current)
  }

  return (
    <div ref={rootRef} className={wide ? css.root : `${css.root} ${css.rail}`} onKeyDown={onKeyDown}>
      {open && anchor !== undefined && (
        <section className={css.panel} style={anchor} aria-label={t('panel.aria')}>
          <section className={css.section}>
            <h3 className={css.sectionTitle}>{t('notices.title')}</h3>
            {liveNotices.length === 0
              ? <p className={css.empty}>{t('notices.empty')}</p>
              : (
                <ul className={css.noticeList}>
                  {liveNotices.map((notice: NotificationView) => (
                    <li
                      key={notice.id}
                      className={notice.read ? css.noticeRow : `${css.noticeRow} ${css.noticeUnread}`}
                    >
                      <button
                        type="button"
                        className={css.noticeMain}
                        title={notice.title}
                        onClick={() => {
                          if (!notice.read) void controller.markRead(notice.id)
                        }}
                      >
                        <span className={css.noticeHead}>
                          <span className={css.noticeTitle}>{notice.title}</span>
                          <span className={css.noticeTime}>{relativeTime(notice.createdAt, now, t)}</span>
                        </span>
                        {notice.body
                          ? <span className={css.noticeBody}>{notice.body}</span>
                          : null}
                      </button>
                      <button
                        type="button"
                        className={css.noticeDismiss}
                        aria-label={t('notice.dismiss')}
                        onClick={() => { void controller.dismissNotice(notice.id) }}
                      >
                        <IconCloseFill14 size={12} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            {noticesErrorLine !== null && <p className={css.error} role="alert">{noticesErrorLine}</p>}
          </section>
          <section className={css.section}>
            <h3 className={css.sectionTitle}>{t('sdk.title')}</h3>
            <div className={css.card}>
              {successTag !== null
                ? (
                  <div className={css.success}>
                    <p className={css.successText}>{t('sdk.installedPendingRestart', { tag: successTag })}</p>
                    {latest !== null && (
                      <a
                        className={css.releaseNotes}
                        href={latest.url}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        {t('sdk.releaseNotes')}
                      </a>
                    )}
                  </div>
                )
                : (
                  <dl className={css.facts}>
                    <dt className={css.factLabel}>{t('sdk.installed.label')}</dt>
                    <dd className={css.factValue}>{installedTag ?? t('sdk.installed.none')}</dd>
                    <dt className={css.factLabel}>{t('sdk.latest.label')}</dt>
                    <dd className={css.factValue}>{latest?.tag ?? t('sdk.latest.none')}</dd>
                  </dl>
                )}
              <div className={css.cardActions}>
                {offerUpdate && (
                  <Button
                    variant="primary"
                    size="sm"
                    disabled={busy}
                    onClick={() => { void controller.install(latest.tag) }}
                  >
                    {snapshot.installing ? t('sdk.installing') : t('sdk.install', { tag: latest.tag })}
                  </Button>
                )}
                {offerUpdate && (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={busy}
                    onClick={() => { void controller.ignore(latest.tag) }}
                  >
                    {t('sdk.skip')}
                  </Button>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={busy}
                  onClick={() => { void controller.checkNow() }}
                >
                  {snapshot.checking ? t('sdk.checking') : t('sdk.check')}
                </Button>
              </div>
              {sdkErrorLine !== null && <p className={css.error} role="alert">{sdkErrorLine}</p>}
            </div>
          </section>
        </section>
      )}
      <button
        ref={triggerRef}
        type="button"
        className={css.trigger}
        aria-label={t('trigger.label')}
        aria-expanded={open}
        onClick={toggle}
      >
        <span className={css.iconWrap}>
          <IconBellOutline16 size={wide ? 16 : 18} />
          {unreadCount > 0 && <span className={css.badge}>{unreadCount}</span>}
          {updateDot && <span className={css.dot} data-update-dot="" />}
        </span>
        {wide && <span className={css.triggerLabel}>{t('trigger.label')}</span>}
      </button>
    </div>
  )
}
