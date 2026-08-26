/**
 * The sidebar Team section: the agents you work with, live members first.
 * Members are the session list's subagent rows — who exists right now, running
 * or inactive, each opening its conversation on click. Below them the roster:
 * the deployment's agent presets rendered as startable teammates (trust badge,
 * description, broken state surfaced honestly); "Start session" creates a
 * session and mounts that preset on it. Roster data arrives through one
 * snapshot store fed by the agentPresets wire face; member data rides the
 * standard global hook — no second subscription anywhere.
 */
import { useState } from 'react'
import clsx from 'clsx'
import { Tooltip } from '@deepseek-ai/dsh-client-ui-primitives'
import type {
  HostObservable, PropsHooks, PropsLocale, PropsRuntime,
} from '@deepseek-ai/dsh-client-ui-slots'
import type {
  SessionId, SessionSummary,
} from '@deepseek-ai/dsh-client-runtime/client'
// Type-only: pulls ui-sidebar's SlotMap merge (the 'sidebar.section' entry).
import type {} from '@deepseek-ai/dsh-client-ui-sidebar/client'
import css from './TeamSection.module.css'

/** One startable teammate: the roster row the section renders. */
export interface TeamPreset {
  /** Preset id, handed back to {@link TeamSectionInjected.startWith}. */
  id: string
  /** Whether the preset ships with the deployment or was authored locally. */
  trust: 'system' | 'user'
  /** Display name the preset published; the id is the fallback. */
  name?: string
  /** One sentence on what the preset is for. */
  description?: string
  /** Why the preset cannot compose a session, absent when it can. */
  broken?: string
}

/** The roster store's snapshot. */
export type TeamRosterState =
  | { status: 'idle' | 'loading' }
  | { status: 'ready'; presets: readonly TeamPreset[] }
  | { status: 'error'; error: string }

/** The section's registrant-private injected share. */
export type TeamSectionInjected = {
  hooks: {
    /** The preset roster snapshot store, bound by the slot renderer. */
    roster: HostObservable<TeamRosterState>
  }
  /** Open a member's conversation (its subagent address, or the session itself). */
  openMember: (sessionId: SessionId) => void
  /**
   * Start a New Session mounted on the named preset. Rejects when the connect
   * or the preset adoption fails after the runtime logged it — render the
   * reason near the trigger.
   */
  startWith: (presetId: string) => Promise<void>
}

/** Full component props: the keyed runtime share + injected share + locale seat. */
export type TeamSectionProps =
  & PropsRuntime<'sidebar.section', 'team'>
  & Omit<TeamSectionInjected, 'hooks'>
  & PropsHooks<TeamSectionInjected['hooks']>
  & PropsLocale<'team'>

/** Members cap: the newest subagent sessions the section renders. */
const MEMBER_LIMIT = 30

/** Compact relative time, mirroring the Home section's buckets. */
function timeLabel(updatedAt: number, now: number, t: TeamSectionProps['t']): string {
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

/**
 * Render the Team section.
 * @param props - composed slot props (keyed runtime share + injected share + locale seat).
 * @returns the Team element tree.
 */
export function TeamSection({
  useSessions,
  useRoster,
  openMember,
  startWith,
  t,
}: TeamSectionProps) {
  const sessions = useSessions(s => s)
  const roster = useRoster(state => state)
  // Per-trigger outcome feedback: starting a teammate can take a while or
  // fail; neither may read as a dead button.
  const [startingId, setStartingId] = useState<string | null>(null)
  const [startError, setStartError] = useState<string | null>(null)
  const handleStartWith = (presetId: string): void => {
    if (startingId !== null) return
    setStartingId(presetId)
    setStartError(null)
    void Promise.resolve(startWith(presetId)).then(
      () => { setStartingId(null) },
      (reason: unknown) => {
        setStartingId(null)
        setStartError(reason instanceof Error ? reason.message : String(reason))
      },
    )
  }

  // Members: the list's subagent rows, running first, then newest. A pure
  // derivation over the standard hook data (rule 5).
  const now = Date.now()
  const members = sessions.ids
    .map(id => sessions.byId[id])
    .filter((row): row is SessionSummary => row?.origin === 'subagent')
    .sort((a, b) => Number(b.running) - Number(a.running) || b.updatedAt - a.updatedAt)
    .slice(0, MEMBER_LIMIT)
  const runningCount = members.filter(row => row.running).length

  return (
    <div className={css.root}>
      <div className={css.sectionTitle}>
        {t('team.members.title')}
        {members.length > 0 && (
          <span className={clsx(css.count, runningCount > 0 && css.countAccent)}>
            {runningCount > 0 ? runningCount : members.length}
          </span>
        )}
      </div>
      {members.length === 0
        ? <p className={css.empty}>{t('team.members.empty')}</p>
        : (
          <ul className={css.memberList}>
            {members.map((row) => {
              const label = row.running ? t('team.member.running') : t('team.member.inactive')
              return (
                <li key={row.id}>
                  <button
                    type="button"
                    className={clsx(css.memberRow, sessions.current === row.id && css.rowCurrent)}
                    onClick={() => { openMember(row.id) }}
                  >
                    <Tooltip label={label} delayMs={500}>
                      <span className={clsx(css.dot, row.running ? css.dotRunning : css.dotIdle)} aria-hidden="true" />
                    </Tooltip>
                    <span className={css.memberTitle}>{row.displayTitle}</span>
                    <span className={css.memberTime}>{timeLabel(row.updatedAt, now, t)}</span>
                  </button>
                </li>
              )
            })}
          </ul>
        )}

      <div className={css.sectionTitle}>{t('team.roster.title')}</div>
      {roster.status === 'error' && (
        <p className={css.empty}>{t('team.roster.error', { message: roster.error })}</p>
      )}
      {(roster.status === 'idle' || roster.status === 'loading') && (
        <p className={css.empty} aria-busy="true">…</p>
      )}
      {roster.status === 'ready' && roster.presets.length === 0 && (
        <p className={css.empty}>{t('team.roster.empty')}</p>
      )}
      {roster.status === 'ready' && roster.presets.length > 0 && (
        <ul className={css.rosterList}>
          {roster.presets.map((preset) => {
            const broken = preset.broken !== undefined
            return (
              <li key={preset.id} className={css.card}>
                <div className={css.cardHead}>
                  <span className={css.cardName}>{preset.name ?? preset.id}</span>
                  <span className={clsx(css.badge, preset.trust === 'system' ? css.badgeSystem : css.badgeUser)}>
                    {preset.trust === 'system' ? t('team.trust.system') : t('team.trust.user')}
                  </span>
                </div>
                {preset.description !== undefined && (
                  <p className={css.cardDescription}>{preset.description}</p>
                )}
                {preset.broken !== undefined && (
                  <p className={css.cardBroken}>{t('team.roster.broken', { reason: preset.broken })}</p>
                )}
                {!broken && (
                  <button
                    type="button"
                    className={css.startButton}
                    aria-label={t('team.start')}
                    aria-busy={startingId === preset.id || undefined}
                    onClick={() => { handleStartWith(preset.id) }}
                  >
                    {startingId === preset.id ? '…' : t('team.start')}
                  </button>
                )}
              </li>
            )
          })}
        </ul>
      )}
      {startError !== null && (
        <p className={css.startError} role="alert">{t('team.start.failed', { message: startError })}</p>
      )}
    </div>
  )
}
