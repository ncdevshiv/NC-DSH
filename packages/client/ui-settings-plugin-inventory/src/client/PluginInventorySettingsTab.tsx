import { useCallback, useEffect, useId, useMemo, useState, type ReactNode } from 'react'
import type { PluginInventorySnapshot } from '@deepseek-ai/dsh-api-remotes/client'
import {
  IconChevronDownOutline14,
  IconSearchOutline16,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { InjectFace, PropsLocale, PropsRuntime } from '@deepseek-ai/dsh-client-ui-slots'
import type { PluginInventoryLocaleKey } from './locales.ts'
import { getPluginMeta, isEssentialPlugin } from './plugin-catalog.ts'
import css from './PluginInventorySettingsTab.module.css'

/** Registration-side Remote face used by the section. */
export interface PluginInventorySettingsTabInjected {
  /** Read a current Host inventory snapshot. */
  list: () => Promise<PluginInventorySnapshot>
  /** Toggle one entry's enablement. */
  setEnabled: (entryId: string, enabled: boolean) => Promise<{ ok: boolean; message?: string }>
}

type PluginInventoryEntry = PluginInventorySnapshot['entries'][number]
type PluginFiberPhase = PluginInventoryEntry['fiberPhase']

/** Full component props assembled by the Settings slot renderer. */
export type PluginInventorySettingsTabProps =
  PropsRuntime<'settings.plugins.tab'>
  & PropsLocale<'settings.pluginInventory'>
  & InjectFace<PluginInventorySettingsTabInjected>

type ViewState =
  | { readonly status: 'loading' }
  | { readonly status: 'error' }
  | { readonly status: 'ready'; readonly snapshot: PluginInventorySnapshot }

const PHASE_KEYS = {
  pending: 'pending',
  loading: 'loadingPhase',
  active: 'active',
  failed: 'failed',
  unloading: 'unloading',
} satisfies Record<Exclude<PluginFiberPhase, null>, PluginInventoryLocaleKey>

/** Localized accessible label for one root Fiber phase. */
function phaseLabel(
  phase: PluginFiberPhase,
  t: PluginInventorySettingsTabProps['t'],
): string {
  return phase === null ? t('unobserved') : t(PHASE_KEYS[phase])
}

/** Compact a module specifier without guessing whether its Loader id was generated. */
function moduleShortName(moduleName: string): string {
  const unscoped = moduleName.startsWith('@') ? moduleName.slice(moduleName.indexOf('/') + 1) : moduleName
  return unscoped
    .replace(/^cordis:/, '')
    .replace(/^cordis-plugin-/, '')
    .replace(/^dsh-(?:host-|client-)?/, '')
}

/** Whether an inventory row matches the local catalog query. */
function matches(entry: PluginInventoryEntry, normalizedQuery: string): boolean {
  if (normalizedQuery.length === 0) return true
  return [entry.moduleName, entry.entryId]
    .some(value => value.toLocaleLowerCase().includes(normalizedQuery))
}

/** Render the Loader inventory with inline enable/disable and per-plugin summaries. */
export function PluginInventorySettingsTab({ list, setEnabled, t }: PluginInventorySettingsTabProps): ReactNode {
  const catalogId = useId()
  const [request, setRequest] = useState(0)
  const [query, setQuery] = useState('')
  const [expanded, setExpanded] = useState<PluginInventoryEntry['entryId'] | null>(null)
  const [state, setState] = useState<ViewState>({ status: 'loading' })
  const [pending, setPending] = useState<ReadonlySet<string>>(() => new Set())
  const [errors, setErrors] = useState<ReadonlyMap<string, string>>(() => new Map())

  useEffect(() => {
    let current = true
    void Promise.resolve().then(() => list()).then(
      (snapshot) => { if (current) setState({ status: 'ready', snapshot }) },
      () => { if (current) setState({ status: 'error' }) },
    )
    return () => { current = false }
  }, [list, request])

  const normalizedQuery = query.trim().toLocaleLowerCase()
  const filteredEntries = useMemo(
    () => state.status === 'ready'
      ? state.snapshot.entries.filter(entry => matches(entry, normalizedQuery))
      : [],
    [normalizedQuery, state],
  )

  useEffect(() => {
    if (expanded !== null && !filteredEntries.some(entry => entry.entryId === expanded)) {
      setExpanded(null)
    }
  }, [expanded, filteredEntries])

  const retry = useCallback((): void => {
    setState({ status: 'loading' })
    setRequest(value => value + 1)
  }, [])

  const refresh = useCallback(async (): Promise<void> => {
    try {
      const snapshot = await list()
      setState({ status: 'ready', snapshot })
    } catch {
      setState({ status: 'error' })
    }
  }, [list])

  const toggle = useCallback(async (entry: PluginInventoryEntry): Promise<void> => {
    const entryId = entry.entryId as string
    if (pending.has(entryId)) return
    const nextEnabled = !entry.enabled
    setPending(prev => new Set([...prev, entryId]))
    setErrors((prev) => {
      const next = new Map(prev)
      next.delete(entryId)
      return next
    })
    // Optimistic flip so the tag and switch answer immediately.
    setState((prev) => {
      if (prev.status !== 'ready') return prev
      return {
        status: 'ready',
        snapshot: {
          entries: prev.snapshot.entries.map(item =>
            item.entryId === entry.entryId ? { ...item, enabled: nextEnabled, fiberPhase: nextEnabled ? item.fiberPhase : null } : item,
          ),
        },
      }
    })
    try {
      const result = await setEnabled(entryId, nextEnabled)
      if (!result.ok) {
        // Revert optimistic change.
        setState((prev) => {
          if (prev.status !== 'ready') return prev
          return {
            status: 'ready',
            snapshot: {
              entries: prev.snapshot.entries.map(item =>
                item.entryId === entry.entryId ? { ...item, enabled: entry.enabled, fiberPhase: entry.fiberPhase } : item,
              ),
            },
          }
        })
        setErrors(prev => new Map(prev).set(entryId, result.message ?? 'toggle failed'))
        return
      }
      await refresh()
    } catch (error) {
      setState((prev) => {
        if (prev.status !== 'ready') return prev
        return {
          status: 'ready',
          snapshot: {
            entries: prev.snapshot.entries.map(item =>
              item.entryId === entry.entryId ? { ...item, enabled: entry.enabled, fiberPhase: entry.fiberPhase } : item,
            ),
          },
        }
      })
      setErrors(prev => new Map(prev).set(entryId, error instanceof Error ? error.message : String(error)))
    } finally {
      setPending((prev) => {
        const next = new Set(prev)
        next.delete(entryId)
        return next
      })
    }
  }, [pending, refresh, setEnabled])

  return (
    <div className={css.section} aria-busy={state.status === 'loading'}>
      {state.status === 'loading' ? <p className={css.status}>{t('loading')}</p> : null}
      {state.status === 'error' ? (
        <div className={css.failure}>
          <p role="alert">{t('error')}</p>
          <button type="button" onClick={retry}>{t('retry')}</button>
        </div>
      ) : null}
      {state.status === 'ready' ? (
        <div className={css.catalog}>
          <label className={css.search}>
            <IconSearchOutline16 aria-hidden="true" />
            <span className={css.visuallyHidden}>{t('search')}</span>
            <input
              type="search"
              value={query}
              placeholder={t('search')}
              aria-label={t('search')}
              onChange={(event) => { setQuery(event.currentTarget.value) }}
            />
          </label>
          <div className={css.catalogHeading}>
            <h3>{t('catalog')}</h3>
            <span data-plugin-count={filteredEntries.length}>{filteredEntries.length}</span>
          </div>
          {state.snapshot.entries.length === 0 ? <p className={css.status}>{t('empty')}</p> : null}
          {state.snapshot.entries.length > 0 && filteredEntries.length === 0
            ? <p className={css.status}>{t('emptySearch')}</p>
            : null}
          {filteredEntries.length > 0 ? (
            <ul className={css.cards}>
              {filteredEntries.map((entry) => {
                const status = phaseLabel(entry.fiberPhase, t)
                const title = moduleShortName(entry.moduleName)
                const configuration = t(entry.enabled ? 'enabledTag' : 'disabledTag')
                const open = expanded === entry.entryId
                const detailId = `${catalogId}-details-${encodeURIComponent(entry.entryId as string)}`
                const meta = getPluginMeta(entry.moduleName)
                const essential = isEssentialPlugin(entry.moduleName)
                const isPending = pending.has(entry.entryId as string)
                const error = errors.get(entry.entryId as string)
                return (
                  <li
                    className={css.card}
                    key={entry.entryId}
                    data-plugin-entry={entry.entryId}
                    data-open={open ? 'true' : undefined}
                    data-enabled={entry.enabled ? 'true' : 'false'}
                  >
                    <div className={css.cardHeader}>
                      <button
                        className={css.cardMain}
                        type="button"
                        aria-expanded={open}
                        aria-controls={detailId}
                        aria-label={entry.enabled ? `${title}, ${status}, ${configuration}` : `${title}, ${configuration}`}
                        onClick={() => {
                          setExpanded(current => current === entry.entryId ? null : entry.entryId)
                        }}
                      >
                        <span className={css.cardTitleWrap}>
                          <strong className={css.cardTitle} title={entry.moduleName}>{title}</strong>
                          <span className={css.cardModule} title={entry.moduleName}>{entry.moduleName}</span>
                        </span>
                        <span className={css.cardTrailing}>
                          {entry.enabled ? (
                            <span
                              className={css.statusDot}
                              data-phase={entry.fiberPhase ?? 'unobserved'}
                              role="img"
                              aria-label={status}
                              title={status}
                            />
                          ) : null}
                          <span className={css.configTag} data-enabled={entry.enabled ? 'true' : 'false'}>
                            {configuration}
                          </span>
                          <IconChevronDownOutline14 className={css.chevron} size={12} aria-hidden="true" />
                        </span>
                      </button>
                      <button
                        type="button"
                        role="switch"
                        aria-checked={entry.enabled}
                        aria-label={entry.enabled ? t('enabledHint') : t('disabledHint')}
                        className={css.toggle}
                        data-enabled={entry.enabled ? 'true' : 'false'}
                        data-pending={isPending ? 'true' : undefined}
                        disabled={isPending}
                        onClick={() => { void toggle(entry) }}
                      >
                        <span className={css.toggleKnob} />
                      </button>
                    </div>
                    {open ? (
                      <div className={css.cardDetails} id={detailId}>
                        <div className={css.meta}>
                          <p className={css.summary}>
                            <span className={css.metaLabel}>{t('summary')}</span>
                            <span className={css.metaText}>{meta.summary}</span>
                          </p>
                          <p className={css.impact}>
                            <span className={css.metaLabel}>{t('impact')}</span>
                            <span className={css.metaText}>{meta.impact}</span>
                          </p>
                          {essential ? <p className={css.essential} role="note">{t('essentialWarning')}</p> : null}
                        </div>
                        <code className={css.entryValue} data-loader-entry>{entry.entryId}</code>
                        <dl className={css.details}>
                          <div>
                            <dt>{t('configuration')}</dt>
                            <dd>{configuration}</dd>
                          </div>
                          {entry.enabled ? (
                            <div>
                              <dt>{t('cordis')}</dt>
                              <dd>{status}</dd>
                            </div>
                          ) : null}
                        </dl>
                        <div className={css.detailActions}>
                          <button
                            type="button"
                            className={css.toggleButton}
                            data-enabled={entry.enabled ? 'true' : 'false'}
                            disabled={isPending}
                            onClick={() => { void toggle(entry) }}
                          >
                            {isPending ? t('toggling') : entry.enabled ? t('toggleDisable') : t('toggleEnable')}
                          </button>
                          {error ? <span className={css.toggleError} role="alert">{(t as unknown as (k: string, p?: Record<string, string>) => string)('toggleFailed', { message: error })}</span> : null}
                        </div>
                      </div>
                    ) : null}
                    {error && !open ? <span className={css.inlineError} role="alert">{error}</span> : null}
                  </li>
                )
              })}
            </ul>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}
