/**
 * The model list of one pi-ai provider profile, plus the actions that ask the
 * provider what it serves and probe one model's connectivity.
 *
 * The list shows everything the route serves: the rows the user layer owns,
 * followed by the models the route's installed catalog serves that no owned row
 * replaces. A catalog row is adjusted through the profile's `modelOverrides`
 * entry for its id — the mechanism the adapter merges over its own catalog — so
 * one model's capacity or input types can be changed without rewriting the
 * whole served list. An owned `models` array still replaces the catalog
 * outright, and only then does it name every row.
 *
 * Fetching asks the endpoint **the form currently shows** — including a key
 * typed but not yet saved — and adopts the complete reply in one pass: a row
 * already served is left untouched, and every model the provider reported that
 * nothing serves yet joins the list. A provider that cannot be interrogated (an
 * unreachable endpoint, a protocol with no readable listing) is not a dead end:
 * the failure is shown next to the rows the user can still fill in by hand.
 *
 * A search field narrows the rows as the user types, matching a model's id or
 * its display name. Filtering only hides rows: a hidden row keeps its place in
 * the served list, so its number, its edits, and its removal still address the
 * row it names.
 */

import { useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import type { LlmDiscoveredModel } from '@deepseek-ai/dsh-api-remotes/client'
import { IconPlusOutlineRegular } from '@deepseek-ai/dsh-client-ui-primitives'
import { formatCapacity, parseCapacity } from './DeepSeekModelsEditor.tsx'
import { mergeDiscoveredModels, MODEL_SOURCE_FIELD } from './mutations.ts'
import { ModelCatalogSearch } from './ModelCatalogSearch.tsx'
import { ModelListActions } from './ModelListActions.tsx'
import { modelMatches } from './model-search.ts'
import type { ModelTestFeedback, ModelsOperations } from './operations.ts'
import type { DeepSeekModelDraft } from './DeepSeekModelsEditor.tsx'
import type { en } from './locales.ts'
import { ModelRow, modelProbeProps } from './ModelRow.tsx'
import type { ModelCapabilities } from './ModelRow.tsx'
import styles from './ModelsSection.module.css'

/**
 * One configured model row. Fields this card does not edit must survive an
 * edit rather than being dropped by a rebuild.
 */
export type ModelDraft = DeepSeekModelDraft

/** A row's text field, or the empty string when unset or not a string. */
function textOf(model: ModelDraft, key: string): string {
  const value = model[key]
  return typeof value === 'string' ? value : ''
}

/** A row's numeric field, or `undefined` when unset or not a number. */
function numberOf(model: ModelDraft, key: string): number | undefined {
  const value = model[key]
  return typeof value === 'number' ? value : undefined
}

/** What an interrogation needs, taken from the live form. */
export interface ProbeTarget {
  /** Settings namespace whose adapter family answers. */
  settingsNs: string
  /**
   * Route being edited, when the card edits one. An adapter that already
   * describes it answers from its own registry, so such a card can ask without
   * an endpoint at all.
   */
  provider?: string
  /** Endpoint as the form currently shows it. */
  baseURL?: string
  /** Wire protocol the form names, when it names one. */
  api?: string
  /** Key typed into the form and not yet stored, when there is one. */
  apiKey?: string
}

/** One listed row: an owned row, or one the route's installed catalog serves. */
interface ServedRow {
  /** The row as it will read, catalog values under the user layer's override. */
  readonly model: ModelDraft
  /** Whether the installed catalog supplies this row and the user layer only adjusts it. */
  readonly catalog: boolean
}

/** The line one fetch or catalog read leaves under the list. */
interface ListNote {
  readonly tone: 'ok' | 'info' | 'error'
  readonly text: string
}

/** Props of {@link ModelListEditor}. */
export interface ModelListEditorProps {
  /** The rows the user layer owns, as currently drafted. */
  models: readonly ModelDraft[]
  /**
   * The adjustments the user layer holds for catalog models, by model id: the
   * fields the adapter merges over its own catalog entry. Absent on a card
   * whose route has no catalog behind it, where no row can carry one.
   */
  overrides?: Readonly<Record<string, ModelDraft>> | undefined
  /** Installed provider whose catalog supplies defaults without endpoint I/O. */
  catalogProvider?: string | undefined
  /**
   * Reasoning levels a hand-declared route's rows may declare in
   * `reasoningEfforts`, in escalation order; absent or empty shows no
   * reasoning control, leaving a catalog model's levels to the catalog.
   */
  reasoningLevels?: readonly string[] | undefined
  /** Route input types for models absent from the installed catalog. */
  defaultInput?: readonly string[] | undefined
  /** Whether the user layer currently owns the served rows; absent on a create. */
  overridden?: boolean
  /** Replace the drafted owned rows. */
  onChange: (models: ModelDraft[]) => void
  /** Replace the drafted catalog adjustments; absent where the route has no catalog. */
  onOverridesChange?: ((overrides: Record<string, ModelDraft>) => void) | undefined
  /** Remove the user-owned rows and adjustments, returning to the catalog; absent on a create. */
  onReset?: () => void
  /** Endpoint facts for the fetch action. */
  probe: ProbeTarget
  /**
   * Copy key naming why the fetch action is unavailable, or `undefined` when
   * it is. The card owns this because the key it would send is judged there:
   * asking with a key the form has already refused spends a round trip to be
   * told what the field already says.
   */
  probeBlocked?: keyof typeof en | undefined
  /** The Host operations whose interrogation answers the fetch action. */
  operations: ModelsOperations
  /** Section copy. */
  t: (key: keyof typeof en) => string
  /** Disable every control (read-only deployment or a pending write). */
  disabled: boolean
  /**
   * Called once per change with whether an endpoint interrogation is in
   * flight. The owning card folds it into its own busy state so the surface
   * around the card — a mode switch, say — can refuse to move while the
   * answer is still bound for this list.
   */
  onBusyChange: (busy: boolean) => void
  /** Probe one model; absent when the card has no test surface. */
  onTestModel?: ((modelId: string) => void) | undefined
  /** Probe several models at once; absent when the card has no test surface. */
  onTestAll?: ((modelIds: readonly string[]) => void) | undefined
  /** Model ids whose probes are in flight. */
  testing?: ReadonlySet<string> | undefined
  /** Each row's settled probe, by model id. */
  testFeedback?: ReadonlyMap<string, ModelTestFeedback> | undefined
}

/** The two token counts edited as K/M-suffixed text behind a row's disclosure. */
type CapacityField = 'contextWindow' | 'maxTokens'

/**
 * What an empty capacity field is worth, shown as its placeholder so a row left
 * blank does not read as a model with no capacity at all.
 *
 * The magnitudes are the adapter's own route-level fallbacks (`llm-pi-ai`'s
 * `defaultContextWindow` and `defaultMaxTokens`), spelled the way a person
 * would say them. They are a hint, not a mirror: this page counts `K` as 1000,
 * so typing `256K` stores 256000 while leaving the field blank keeps the
 * adapter's 262144. A deployment that overrides those defaults is not
 * reflected here — nothing on this page can read them.
 */
const CAPACITY_HINT: Readonly<Record<CapacityField, string>> = {
  contextWindow: '256K',
  maxTokens: '32K',
}

/**
 * Spell a stored count for a field that may be unset. The spelling itself is
 * {@link formatCapacity}, shared with the DeepSeek catalog editor so both
 * surfaces read and write one K/M vocabulary.
 * @param value - stored capacity, or `undefined` for an unset field.
 * @returns the field text, empty when unset.
 */
function capacitySpelling(value: number | undefined): string {
  return value === undefined ? '' : formatCapacity(value)
}

/** The catalog entry one served row reads under, as a comparable draft. */
function catalogDraft(candidate: LlmDiscoveredModel): ModelDraft {
  return {
    id: candidate.id,
    ...candidate.name === undefined ? {} : { name: candidate.name },
    ...candidate.contextWindow === undefined ? {} : { contextWindow: candidate.contextWindow },
    ...candidate.maxTokens === undefined ? {} : { maxTokens: candidate.maxTokens },
    ...candidate.inputModalities === undefined ? {} : { input: [...candidate.inputModalities] },
  }
}

/**
 * Render the model list with its fetch, test, and reset actions.
 * @param props - the drafted rows, probe target, wire face, and copy.
 * @returns the model-list editor.
 */
export function ModelListEditor(props: ModelListEditorProps): ReactNode {
  const { models, onChange, probe, operations, t, disabled, onBusyChange } = props
  const { catalogProvider } = props
  const overrides = props.overrides ?? {}
  const [busy, setBusy] = useState(false)
  useEffect(() => { onBusyChange(busy) }, [busy, onBusyChange])
  const [note, setNote] = useState<ListNote | undefined>(undefined)
  /** The search text over the served rows. */
  const [query, setQuery] = useState('')
  const [inheritedCatalog, setInheritedCatalog] = useState<{
    provider: string
    models: readonly LlmDiscoveredModel[]
  } | undefined>(undefined)
  useEffect(() => {
    if (catalogProvider === undefined) return
    let current = true
    void operations.discoverModels(probe.settingsNs, { provider: catalogProvider }).then((answer) => {
      if (!current) return
      setInheritedCatalog({ provider: catalogProvider, models: answer.kind === 'found' ? answer.models : [] })
      if (answer.kind === 'refused') setNote({ tone: 'error', text: answer.message })
    })
    return () => { current = false }
  }, [catalogProvider, operations, probe.settingsNs])
  const catalog = inheritedCatalog?.provider === catalogProvider ? inheritedCatalog?.models : undefined
  const inputDefaults = useMemo(() => new Map(catalog?.map(model => [model.id, model.inputModalities])), [catalog])
  // Capability facts the listing disclosed, keyed by model id. A row with no
  // fact keeps no chip rather than a guessed one.
  const capabilities = useMemo(() => new Map(catalog?.flatMap((model) => {
    const entry: ModelCapabilities = {
      ...model.reasoning === undefined ? {} : { reasoning: model.reasoning },
      ...model.inputModalities === undefined ? {} : { image: model.inputModalities.includes('image') },
      ...model.contextWindow === undefined ? {} : { contextWindow: model.contextWindow },
    }
    return Object.keys(entry).length === 0 ? [] : [[model.id, entry] as const]
  }) ?? []), [catalog])
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(new Set())
  // Capacities are edited as text, so a field's keystrokes are held here rather
  // than re-derived from the parsed count on every change — that would rewrite
  // `1000` to `1K` mid-word. Unreadable text is kept past blur so the refusal
  // names a row the user can still see, which is why this is one entry PER
  // FIELD: a single buffer would be displaced by editing any other field, and
  // the abandoned one would render its stored NaN as the literal `NaN`.
  const [editing, setEditing] = useState<ReadonlyMap<string, string>>(new Map())

  // The rows the route serves, in the order it serves them: the owned rows
  // first, then the installed catalog's own entries for every model no owned
  // row names. A catalog row reads through the user layer's adjustment for its
  // id, which is the only way the adapter lets one of its models be changed.
  const rows: ServedRow[] = useMemo(() => {
    const owned = models.map(model => ({ model, catalog: false }))
    const named = new Set(models.map(model => textOf(model, 'id')).filter(id => id.length > 0))
    const rest = (catalog ?? []).flatMap((candidate) => {
      if (named.has(candidate.id)) return []
      return [{ model: { ...catalogDraft(candidate), ...overrides[candidate.id] ?? {} }, catalog: true }]
    })
    return [...owned, ...rest]
  }, [models, catalog, overrides])

  // The rows the search term leaves visible. Each keeps its place in the
  // served list, so a filtered row is still the row its number, its edits, and
  // its removal action name.
  const visibleRows = useMemo(
    () => rows.flatMap((row, index) => modelMatches({
      id: textOf(row.model, 'id'),
      name: textOf(row.model, 'name'),
    }, query) ? [{ row, index }] : []),
    [rows, query],
  )

  /** Buffer key for one capacity field; the row half moves when rows do. */
  const bufferKey = (index: number, field: CapacityField): string => `${String(index)}:${field}`

  /**
   * The adjustment one edited catalog row leaves: only the fields that differ
   * from the catalog's own entry, so returning a field to the catalog's value
   * removes it from the profile instead of storing the catalog's number. An
   * enabled row states the catalog's own default and is left out for the same
   * reason.
   */
  const overrideOf = (id: string, next: ModelDraft): ModelDraft => {
    const base = (catalog ?? []).find(candidate => candidate.id === id)
    const from = base === undefined ? {} : catalogDraft(base)
    return Object.fromEntries(Object.entries(next).filter(([key, value]) =>
      key !== 'id'
      && !(key === 'enabled' && value === true)
      && JSON.stringify(value) !== JSON.stringify(from[key])))
  }

  /** Store one row's adjustment, dropping the entry once it says nothing. */
  const setOverride = (id: string, entry: ModelDraft): void => {
    const write = props.onOverridesChange
    /* v8 ignore next -- a catalog row exists only where the card was given the callback that writes it */
    if (write === undefined) return
    const next = { ...overrides }
    if (Object.keys(entry).length === 0) Reflect.deleteProperty(next, id)
    else next[id] = entry
    write(next)
  }

  /** Replace one row, wherever the route's list keeps it. */
  const replaceRow = (row: ServedRow, index: number, next: ModelDraft): void => {
    if (row.catalog) {
      setOverride(textOf(row.model, 'id'), overrideOf(textOf(row.model, 'id'), next))
      return
    }
    onChange(models.map((model, at) => at === index ? next : model))
  }

  /**
   * Apply one field patch to one row. Rebuilt rather than spread over: an
   * emptied optional field has to leave the profile, not be stored as a value
   * its schema would reject.
   */
  const patchRow = (row: ServedRow, index: number, fields: Record<string, string | number | undefined>): void => {
    const cleared = new Set(
      Object.entries(fields).filter(([, value]) => value === undefined || value === '').map(([key]) => key),
    )
    const next = Object.fromEntries(
      Object.entries({ ...row.model, ...fields }).filter(([key]) => !cleared.has(key)),
    )
    replaceRow(row, index, next)
  }

  const editCapacity = (row: ServedRow, index: number, field: CapacityField, text: string): void => {
    setEditing(current => new Map(current).set(bufferKey(index, field), text))
    patchRow(row, index, { [field]: parseCapacity(text) })
  }

  /** What a capacity field shows: the buffer while typing, else the stored count. */
  const capacityText = (model: ModelDraft, index: number, field: CapacityField): string =>
    editing.get(bufferKey(index, field)) ?? capacitySpelling(numberOf(model, field))

  /** Drop one row's entries and shift the rows after it down, in one pass. */
  const reindexOnRemove = (
    current: ReadonlyMap<string, string>,
    index: number,
  ): Map<string, string> => {
    const next = new Map<string, string>()
    for (const [key, value] of current) {
      const at = Number(key.slice(0, key.indexOf(':')))
      if (at === index) continue
      // Only the row number moves; the field half of the key is untouched.
      next.set(at > index ? key.replace(/^\d+/, String(at - 1)) : key, value)
    }
    return next
  }

  const toggleExpanded = (index: number): void => {
    setExpanded((current) => {
      const next = new Set(current)
      if (!next.delete(index)) next.add(index)
      return next
    })
  }

  const removeRow = (index: number): void => {
    onChange(models.filter((_model, at) => at !== index))
    setExpanded((current) => {
      const next = new Set<number>()
      for (const at of current) {
        if (at < index) next.add(at)
        else if (at > index) next.add(at - 1)
      }
      return next
    })
    setEditing(current => reindexOnRemove(current, index))
  }

  /**
   * Ask the provider what it serves and sync the list to the reply. An owned
   * list keeps its tuned fields, gains every model nothing serves yet, and
   * loses every row the endpoint no longer reports; a row the user added by
   * hand is never removed. A route that serves its installed catalog stores no
   * list, so its sync hides the catalog models the reply omits and shows again
   * the ones it reports after an earlier sync hid them.
   */
  const fetchModels = async (): Promise<void> => {
    setBusy(true)
    setNote(undefined)
    try {
      const answer = await operations.discoverModels(probe.settingsNs, {
        ...probe.provider === undefined ? {} : { provider: probe.provider },
        ...probe.baseURL === undefined || probe.baseURL.length === 0 ? {} : { baseURL: probe.baseURL },
        ...probe.api === undefined ? {} : { api: probe.api },
        ...probe.apiKey === undefined ? {} : { apiKey: probe.apiKey },
      })
      if (answer.kind === 'refused') {
        setNote({ tone: 'error', text: answer.message })
        return
      }
      const found = answer.models
      if (catalogProvider !== undefined) setInheritedCatalog({ provider: catalogProvider, models: found })
      if (found.length === 0) {
        setNote({ tone: 'info', text: t('fetchEmpty') })
        return
      }
      const served = rows.map(row => textOf(row.model, 'id')).filter(id => id.length > 0)
      const merged = mergeDiscoveredModels(served, models, found)
      if (merged.added === 0 && merged.removed === 0) {
        setNote({ tone: 'ok', text: t('fetchCompleteList') })
        return
      }
      if (merged.models.length === 0) {
        setNote({ tone: 'info', text: t('fetchEmpty') })
        return
      }
      onChange(merged.models)
      setNote({
        tone: 'ok',
        text: t(merged.added > 0 && merged.removed > 0 ? 'fetchSynced' : merged.removed > 0 ? 'fetchRemoved' : 'fetchAddedModels')
          .replace('{count}', () => String(merged.added))
          .replace('{removed}', () => String(merged.removed)),
      })
    } finally {
      setBusy(false)
    }
  }

  // A route the adapter already describes answers without an endpoint; only a
  // draft with neither has nothing to ask about.
  const askable = probe.provider !== undefined || (probe.baseURL !== undefined && probe.baseURL.length > 0)
  const testable = rows.flatMap(row => textOf(row.model, 'id')).filter(id => id.length > 0)
  return (
    <section className={styles['modelCatalog']} aria-label={t('models')}>
      <div className={styles['modelListHead']}>
        <div className={styles['modelCatalogHeading']}>
          <span className={styles['modelCatalogTitle']}>{t('models')}</span>
          {props.overridden === undefined
            ? null
            : (
              <span className={styles['modelCatalogMeta']}>
                {props.overridden ? t('modelsCustomized') : t('modelsInherited')}
              </span>
            )}
        </div>
        <ModelListActions
          t={t}
          disabled={disabled || busy}
          testing={(props.testing?.size ?? 0) > 0}
          testIds={testable}
          onTestAll={props.onTestAll}
          onReset={props.overridden === true ? props.onReset : undefined}
        >
          <button
            type="button"
            className={styles['linkButton']}
            disabled={disabled || busy || !askable || props.probeBlocked !== undefined}
            title={props.probeBlocked !== undefined
              ? t(props.probeBlocked)
              : askable ? t('fetchOffered') : t('fetchNeedsBaseUrl')}
            onClick={() => { void fetchModels() }}
          >
            {busy ? t('fetching') : t('fetchModels')}
          </button>
        </ModelListActions>
      </div>
      {rows.length === 0
        ? <p className={styles['modelEmpty']}>{t('modelsEmpty')}</p>
        : <ModelCatalogSearch value={query} t={t} onChange={setQuery} />}
      {rows.length > 0 && visibleRows.length === 0
        ? <p className={styles['modelEmpty']}>{t('noMatchingModels')}</p>
        : null}
      <div className={styles['modelList']}>
        {visibleRows.map(({ row, index }) => {
          const model = row.model
          const modelId = textOf(model, 'id')
          return (
            <ModelRow
              key={row.catalog ? `catalog:${modelId}` : `own:${String(index)}`}
              model={model}
              position={index + 1}
              inputField="input"
              catalog={row.catalog}
              inputFallback={row.catalog ? undefined : inputDefaults.get(modelId) ?? props.defaultInput}
              inputLoading={row.catalog ? false : catalogProvider !== undefined && catalog === undefined}
              reasoningLevels={props.reasoningLevels}
              capabilities={capabilities.get(modelId)}
              expanded={expanded.has(index)}
              disabled={disabled}
              t={t}
              contextWindow={{
                value: capacityText(model, index, 'contextWindow'),
                placeholder: CAPACITY_HINT.contextWindow,
                onChange: (text) => { editCapacity(row, index, 'contextWindow', text) },
              }}
              maxTokens={{
                value: capacityText(model, index, 'maxTokens'),
                placeholder: CAPACITY_HINT.maxTokens,
                onChange: (text) => { editCapacity(row, index, 'maxTokens', text) },
              }}
              onFieldChange={(field, value) => { patchRow(row, index, { [field]: value }) }}
              onChange={(next) => { replaceRow(row, index, next) }}
              onToggle={() => { toggleExpanded(index) }}
              {...row.catalog ? {} : { onRemove: () => { removeRow(index) } }}
              {...modelProbeProps(props.onTestModel, props.testing, props.testFeedback, modelId)}
            />
          )
        })}
      </div>
      <button
        type="button"
        className={styles['addModelButton']}
        disabled={disabled}
        onClick={() => {
          // A row added by hand is marked custom so a later fetch never prunes it.
          onChange([...models.map(model => ({ ...model })), { id: '', [MODEL_SOURCE_FIELD]: 'custom' as const }])
        }}
      >
        <IconPlusOutlineRegular size={14} />
        {t('addModel')}
      </button>
      {note === undefined
        ? null
        : (
          <p
            role="status"
            className={styles[note.tone === 'error' ? 'error' : note.tone === 'ok' ? 'savedNotice' : 'advancedHint']}
          >
            {note.text}
          </p>
        )}
    </section>
  )
}
