/**
 * The sidebar Home section: an inbox-style overview. A summary strip (running
 * sessions, total sessions, workspaces) sits above a New Session quick action
 * and the recent-session inbox — non-blank sessions newest-first, each row
 * carrying its state dot (running / needs-you / done), agent-preset label,
 * and compact relative time; a row click opens that session. All data rides
 * the standard global hooks; the only actions are the runtime's shared
 * start/open verbs, injected from the apply closure.
 */
import { useState } from 'react'
import clsx from 'clsx'
import { IconNewChatOutline16, Tooltip } from '@deepseek-ai/dsh-client-ui-primitives'
import type {
  PropsLocale, PropsRuntime,
} from '@deepseek-ai/dsh-client-ui-slots'
import type {
  SessionId, SessionSummary,
} from '@deepseek-ai/dsh-client-runtime/client'
// Type-only: pulls ui-sidebar's SlotMap merge (the 'sidebar.section' entry).
import type {} from '@deepseek-ai/dsh-client-ui-sidebar/client'
import css from './HomeSection.module.css'

/** The section's registrant-private injected share: the shared session verbs. */
export type HomeSectionInjected = {
  /**
   * Start a New Session (current Session Workspace, then recent Workspace).
   * Rejects on connect failure after the runtime logged it — render the
   * reason near the trigger.
   */
  startSession: () => Promise<SessionId>
  /** Open a real Session. */
  open: (sessionId: SessionId) => void
}

/** Full component props: the keyed runtime share + injected verbs + locale seat. */
export type HomeSectionProps =
  & PropsRuntime<'sidebar.section', 'home'>
  & HomeSectionInjected
  & PropsLocale<'home'>

/** Inbox cap: the newest non-blank sessions the overview renders. */
const INBOX_LIMIT = 30

/** Compact relative time ("now" / "5min" / "3h" / "2d"), mirroring the
 *  session rows' buckets. Local to this package: the workspace rows' variant
 *  is internal to ui-workspace and carries its own locale namespace. */
function timeLabel(updatedAt: number, now: number, t: HomeSectionProps['t']): string {
  const delta = Math.max(0, now - updatedAt)
  const minutes = Math.floor(delta / 60_000)
  if (minutes < 1) return t('time.now')
  if (minutes < 60) return t('time.minutes', { n: minutes })
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return t('time.hours', { n: hours })
  const days = Math.floor(hours / 24)
  if (days < 30) return t('time.days', { n: days })
  const months = Math.floor(days / 30)
  if (months < 12) return t('time.months', { n: months })
  return t('time.years', { n: Math.floor(months / 12) })
}

/** Row state: the dot's class and its label key. */
function rowState(session: { running: boolean; pendingInteraction?: unknown; completed?: boolean }, t: HomeSectionProps['t']): {
  dotClass: string | undefined
  label: string | undefined
} {
  if (session.pendingInteraction !== undefined) return { dotClass: css.dotPending, label: t('home.row.pending') }
  if (session.running) return { dotClass: css.dotRunning, label: t('home.row.running') }
  if (session.completed === true) return { dotClass: css.dotDone, label: t('home.row.done') }
  return { dotClass: css.dotIdle, label: undefined }
}

/**
 * Render the Home section.
 * @param props - composed slot props (keyed runtime share + injected verbs + locale seat).
 * @returns the Home element tree.
 */
export function HomeSection({
  useSessions,
  useWorkspaces,
  startSession,
  open,
  t,
}: HomeSectionProps) {
  const sessions = useSessions(s => s)
  const workspaces = useWorkspaces(s => s.items)
  // New Session outcome feedback, the sidebar trigger's pattern: the action
  // can take a while or fail — neither may read as a dead button.
  const [startBusy, setStartBusy] = useState(false)
  const [startError, setStartError] = useState<string | null>(null)
  const handleStartSession = (): void => {
    if (startBusy) return
    setStartBusy(true)
    setStartError(null)
    void Promise.resolve(startSession()).then(
      () => { setStartBusy(false) },
      (reason: unknown) => {
        setStartBusy(false)
        setStartError(reason instanceof Error ? reason.message : String(reason))
      },
    )
  }

  // The inbox: non-blank sessions, newest first, capped. A pure derivation
  // over the standard hook data (rule 5) — no second subscription.
  const now = Date.now()
  const inbox = sessions.ids
    .map(id => sessions.byId[id])
    .filter((row): row is SessionSummary => row !== undefined && !row.blank)
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .slice(0, INBOX_LIMIT)
  const runningCount = sessions.ids
    .filter(id => sessions.byId[id]?.running === true)
    .length

  return (
    <div className={css.root}>
      <div className={css.summary}>
        <span className={clsx(css.summaryChip, runningCount > 0 && css.summaryChipAccent)}>
          {t('home.summary.running', { n: runningCount })}
        </span>
        <span className={css.summaryChip}>{t('home.summary.sessions', { n: sessions.ids.length })}</span>
        <span className={css.summaryChip}>{t('home.summary.workspaces', { n: workspaces.length })}</span>
      </div>

      <button
        type="button"
        className={css.newSession}
        aria-label={t('home.newSession')}
        aria-busy={startBusy || undefined}
        onClick={handleStartSession}
      >
        <IconNewChatOutline16 size={14} />
        <span>{startBusy ? '…' : t('home.newSession')}</span>
      </button>
      {startError !== null && (
        <p className={css.startError} role="alert">{t('home.newSession.failed', { message: startError })}</p>
      )}

      <div className={css.inboxTitle}>{t('home.inbox.title')}</div>
      {inbox.length === 0
        ? <p className={css.inboxEmpty}>{t('home.inbox.empty')}</p>
        : (
          <ul className={css.inbox}>
            {inbox.map((row) => {
              const { dotClass, label } = rowState(row, t)
              return (
                <li key={row.id}>
                  <button
                    type="button"
                    className={clsx(css.row, sessions.current === row.id && css.rowCurrent)}
                    onClick={() => { open(row.id) }}
                  >
                    <Tooltip label={label ?? ''} delayMs={500} disabled={label === undefined}>
                      <span className={clsx(css.dot, dotClass)} aria-hidden="true" />
                    </Tooltip>
                    <span className={css.rowTitle}>{row.displayTitle}</span>
                    {row.agentPreset !== undefined && <span className={css.rowPreset}>{row.agentPreset}</span>}
                    <span className={css.rowTime}>{timeLabel(row.updatedAt, now, t)}</span>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
    </div>
  )
}
