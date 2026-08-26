/**
 * The sidebar Work section: a live board of what the agents are doing.
 * "Needs you" lists the sessions blocked on a user interaction (approvals,
 * questions) — the actionable top of the board. "Running" lists every
 * executing session with its state dot and relative start of recency. Both
 * ride the session-list projection, so they are complete for the whole
 * account. "Goals" renders each session whose `goal` projection value is
 * materialized client-side (opened sessions; the host computes list rows
 * without opening logs) with its phase chip — the cross-session enumeration
 * for unopened sessions is the documented host-RPC follow-up. A row click
 * opens that session; all data is the standard global hook, no second
 * subscription.
 */
import clsx from 'clsx'
import type { PropsLocale, PropsRuntime } from '@deepseek-ai/dsh-client-ui-slots'
import type {
  SessionId, SessionSummary,
} from '@deepseek-ai/dsh-client-runtime/client'
// Type-only: pulls ui-sidebar's SlotMap merge (the 'sidebar.section' entry).
import type {} from '@deepseek-ai/dsh-client-ui-sidebar/client'
// Type-only: the `goal` SessionProjectionMap key merge (single source, the
// domain's pure outlet) — erased, so it creates no module-graph request.
import type { GoalProjection } from '@deepseek-ai/dsh-goal/client'
import css from './WorkSection.module.css'

/** The section's registrant-private injected share: the open verb. */
export type WorkSectionInjected = {
  /** Open a real Session. */
  open: (sessionId: SessionId) => void
}

/** Full component props: the keyed runtime share + injected verb + locale seat. */
export type WorkSectionProps =
  & PropsRuntime<'sidebar.section', 'work'>
  & WorkSectionInjected
  & PropsLocale<'work'>

/** Rows cap per board list. */
const LIST_LIMIT = 20

/** Compact relative time, mirroring the sibling sections' buckets. */
function timeLabel(updatedAt: number, now: number, t: WorkSectionProps['t']): string {
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

/** The goal projection a list row carries, when one is materialized. */
function rowGoal(row: SessionSummary): GoalProjection | null | undefined {
  return (row.projectionValues as { goal?: GoalProjection | null } | undefined)?.goal
}

/**
 * Render the Work section.
 * @param props - composed slot props (keyed runtime share + injected verb + locale seat).
 * @returns the Work element tree.
 */
export function WorkSection({
  useSessions,
  open,
  t,
}: WorkSectionProps) {
  const sessions = useSessions(s => s)
  const now = Date.now()
  const rows = sessions.ids
    .map(id => sessions.byId[id])
    .filter((row): row is SessionSummary => row !== undefined && !row.blank)

  // Needs you: a user interaction is blocking the session. These lead the
  // board because they are the only rows where the human is the bottleneck.
  const attention = rows
    .filter(row => row.pendingInteraction !== undefined)
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .slice(0, LIST_LIMIT)
  // Running: the executing set, complete for the account.
  const running = rows
    .filter(row => row.pendingInteraction === undefined && row.running)
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .slice(0, LIST_LIMIT)
  // Goals: only where the projection is materialized (opened sessions) —
  // sparse by design until the host-side enumeration lands.
  const goals = rows
    .map(row => ({ row, goal: rowGoal(row) }))
    .filter((entry): entry is { row: SessionSummary; goal: GoalProjection } => entry.goal !== undefined && entry.goal !== null)
    .sort((a, b) => b.goal.updatedAt - a.goal.updatedAt)
    .slice(0, LIST_LIMIT)

  const memberRow = (row: SessionSummary, label: string, dotClass: string | undefined) => (
    <li key={row.id}>
      <button
        type="button"
        className={clsx(css.row, sessions.current === row.id && css.rowCurrent)}
        onClick={() => { open(row.id) }}
      >
        <span className={clsx(css.dot, dotClass)} aria-hidden="true" />
        <span className={css.rowTitle}>{row.displayTitle}</span>
        <span className={css.rowState}>{label}</span>
        <span className={css.rowTime}>{timeLabel(row.updatedAt, now, t)}</span>
      </button>
    </li>
  )

  return (
    <div className={css.root}>
      <div className={css.sectionTitle}>{t('work.attention.title')}</div>
      {attention.length === 0
        ? <p className={css.empty}>{t('work.attention.empty')}</p>
        : (
          <ul className={css.list}>
            {attention.map(row => memberRow(row, t('work.row.pending'), css.dotPending))}
          </ul>
        )}

      <div className={css.sectionTitle}>{t('work.running.title')}</div>
      {running.length === 0
        ? <p className={css.empty}>{t('work.running.empty')}</p>
        : (
          <ul className={css.list}>
            {running.map(row => memberRow(row, t('work.row.running'), css.dotRunning))}
          </ul>
        )}

      {goals.length > 0 && (
        <>
          <div className={css.sectionTitle}>{t('work.goals.title')}</div>
          <ul className={css.list}>
            {goals.map(({ row, goal }) => (
              <li key={row.id}>
                <button
                  type="button"
                  className={clsx(css.row, css.goalRow, sessions.current === row.id && css.rowCurrent)}
                  onClick={() => { open(row.id) }}
                >
                  <span className={clsx(
                    css.phaseChip,
                    goal.goal.phase === 'blocked' && css.phaseBlocked,
                    goal.goal.phase === 'paused' && css.phasePaused,
                  )}
                  >
                    {goal.goal.phase === 'blocked'
                      ? t('work.goal.blocked')
                      : goal.goal.phase === 'paused' ? t('work.goal.paused') : t('work.goal.active')}
                  </span>
                  <span className={css.goalObjective}>{goal.goal.objective}</span>
                  <span className={css.rowTime}>{timeLabel(goal.updatedAt, now, t)}</span>
                </button>
                {goal.goal.blockedReason !== undefined && (
                  <p className={css.blockedReason}>{goal.goal.blockedReason.message}</p>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}
