/**
 * Models settings section: one full-width provider list whose selected row
 * expands the provider editor beneath it. The list states whether a provider is
 * ready, paused, needing the user, or misconfigured, and lets the user select,
 * pause, reorder, and delete it. A search box filters rows by product name or
 * route id; a move counts against the full configured order, so a filtered list
 * can never drop a route from the stored `providerOrder`.
 *
 * The add action opens one modal: a mode switch chooses between picking a
 * dormant directory provider (a searchable provider grid over the provider
 * editor) and declaring a custom model API (the create form). A panel mounts the
 * first time its mode is shown and stays mounted, hidden, while the modal is open
 * and its mode stays offered, so switching modes discards neither draft; the
 * switch holds still while either panel has a write or an endpoint interrogation
 * in flight, since a switch underneath one would orphan the answer.
 *
 * Row controls write through the same revision-fenced path operations as the
 * editors and report their settled outcome through the app-wide banner: a
 * pending write shows on its row, success and failure name the provider, and a
 * failure carries the Host diagnostic with a Retry wherever re-issuing the write
 * can land. A refused revision re-reads the page first, so the list shows what
 * the Host holds now instead of replaying a stale write. Every mutation writes
 * through the wire, while a provider removal first requires confirmation; the
 * page re-renders from pushed invalidations or the post-apply reload.
 */

import { useId, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import {
  Button, IconPlusOutlineRegular, IconWarningOutlineRegular, Modal, SegmentedControl, Toast,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { InjectFace, PropsRenderSlots } from '@deepseek-ai/dsh-client-ui-slots'
import type { SettingsNamespaceView } from '@deepseek-ai/dsh-api-remotes/client'
// Type-only: pulls this package's SlotMap merge (the two Models child slots).
import type {} from './slot-contract.ts'
import { CustomProviderCard } from './CustomProviderCard.tsx'
import { modelDrafts } from './DeepSeekModelsEditor.tsx'
import { ModelOrderCard } from './ModelOrderCard.tsx'
import { mergeDiscoveredModels, modelOrderSyncOps, modelsArrayOps } from './mutations.ts'
import { ReorderList } from './ReorderList.tsx'
import type { RowAction } from './ReorderList.tsx'
import {
  protocolChoices, providerUsable, reasoningLevelChoices,
} from './store.ts'
import type { ModelsSettingsStore, ProviderRow } from './store.ts'
import {
  fetchProbe, modelOrderWrite, movableProviderIds, pauseWrite, providerCopy,
  providerOrderWrite, railItem, rowMatches, servedModelIds, targetOf,
} from './row-controls.ts'
import type {
  EditorTarget, FetchProbe, ProviderIdentity, RailItem, RowWrite,
} from './row-controls.ts'
import type { ModelsOperations } from './operations.ts'
import type { SettingsSchemaOperations } from './schema-operations.ts'
import { ProviderEditor, type ProviderEditorProps } from './ProviderEditor.tsx'
import type { en } from './locales.ts'
import styles from './ModelsSection.module.css'

/** Injected dependencies of {@link ModelsSection} (slot `inject`). */
export interface ModelsSectionInjected {
  /** The page store (loaded on mount, refreshed on pushed invalidations). */
  controller: ModelsSettingsStore
  hooks: {
    /** Page snapshot bound by the UI renderer as useSnapshot. */
    snapshot: ModelsSettingsStore['store']
  }
  /** The Host operations the section and its cards invoke. */
  operations: ModelsOperations
  /** Settings schema and immutable path callbacks. */
  schema: SettingsSchemaOperations
  /** Section copy. */
  t: (key: keyof typeof en) => string
}

/**
 * The two ways the add modal gains a provider: adopt a directory row the
 * adapter already knows, or declare a route it does not.
 */
type AddMode = 'catalog' | 'custom'

/** The child slots this section declares and dispatches (see ./slot-contract.ts). */
type ModelsChildSlots = 'settings.models.provider-card' | 'settings.models.footer'

/** The child-slot dispatch function the renderer binds for the section. */
type ModelsRenderSlot = PropsRenderSlots<ModelsChildSlots>['renderSlot']

/**
 * Props delivered by the slot outlet: the inject face spread flat (the
 * renderer erases the share boundary at the render call) plus the child-slot
 * dispatch seat. The seat is required: the renderer binds it at the render
 * call itself — unlike the inject face it is never absent at runtime — and a
 * direct render that forgets it fails to compile instead of mounting nothing.
 */
export type ModelsSectionProps = Partial<InjectFace<ModelsSectionInjected>> & PropsRenderSlots<ModelsChildSlots>

type ModelsSectionFace = InjectFace<ModelsSectionInjected>

/** A dormant directory row the add modal can adopt, with its registered namespace. */
interface AddableRow {
  row: ProviderRow
  namespace: SettingsNamespaceView
}

/** One configured row with the namespace that takes its writes and the models it serves. */
interface ConfiguredRow {
  modelIds: readonly string[]
  namespace: SettingsNamespaceView
  row: ProviderRow
  target: EditorTarget
}

/** The catalog draft the add modal shows: its editor target and the namespace that takes the write. */
interface CatalogDraft {
  target: EditorTarget
  namespace: SettingsNamespaceView
}

/** Values that vary around the shared provider-editor rendering. */
interface ProviderEditorRenderProps extends Pick<
  ProviderEditorProps,
  'namespace' | 'schema' | 'operations' | 't' | 'readOnly' | 'onClose'
> {
  target: EditorTarget
  row: ProviderRow
}

/**
 * One settled row-level write, announced through the app-wide banner.
 * `seq` keys the banner, so reporting the same outcome twice restarts it.
 */
type RowNotice = RowNoticeBase & (
  /** A save landed. */
  | { readonly kind: 'saved' }
  /** A fetch synced the list; the counts say what moved. */
  | {
    readonly kind: 'fetched'
    /** Rows the fetch adopted. */
    readonly added: number
    /** Stale fetched rows the sync pruned. */
    readonly removed: number
    /** The endpoint listed no models, so nothing was adopted or pruned. */
    readonly empty: boolean
  }
  /** A write was refused. */
  | { readonly kind: 'failed' }
)

/** The facts every settled row write carries. */
interface RowNoticeBase {
  /** Host diagnostic; empty for a write that landed. */
  readonly detail: string
  /** The write a Retry re-issues; absent where the Host refused the value itself. */
  readonly retry: RowWrite | undefined
  readonly seq: number
  /** Whether a stale revision was refused, which re-reads the page first. */
  readonly stale: boolean
  readonly target: ProviderIdentity
}

/** Omit that keeps a union's members apart instead of merging their fields. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never

/** One write's outcome before the banner sequence is attached. */
type RowNoticeInput = DistributiveOmit<RowNotice, 'seq'>

/** The banner's resolved view of one settled write. */
interface ShownNotice {
  readonly holdMs: number
  readonly identity: ProviderIdentity
  readonly kind: RowNotice['kind']
  /** The write a Retry re-issues; absent where the Host refused the value itself. */
  readonly retry: RowWrite | undefined
  readonly seq: number
  readonly text: string
}

/** Full-opacity banner hold for a write the row itself confirms. */
const SUCCESS_HOLD_MS = 3000
/** Full-opacity banner hold for a failure that names what broke and how to recover. */
const FAILURE_HOLD_MS = 9000

/** Render an editor for the selected provider row, or for the add modal's draft. */
function renderProviderEditor({ target, row, ...props }: ProviderEditorRenderProps): ReactNode {
  return (
    <ProviderEditor
      provider={target.provider}
      displayName={target.displayName}
      settingsPath={target.settingsPath}
      {...target.declared === true ? { declared: true } : {}}
      {...target.defaultBaseURL === undefined ? {} : { defaultBaseURL: target.defaultBaseURL }}
      {...target.baseURLEnv === undefined ? {} : { baseURLEnv: target.baseURLEnv }}
      {...target.nativeAuth === undefined ? {} : { nativeAuth: target.nativeAuth }}
      {...row.credential === undefined ? {} : { credential: row.credential }}
      {...row.credentialChecks === undefined ? {} : { credentialChecks: row.credentialChecks }}
      {...props}
    />
  )
}

/** Props of {@link CatalogPicker}. */
interface CatalogPickerProps {
  /** Dormant directory rows the add flow may adopt. */
  rows: readonly ProviderRow[]
  /** Search text as typed. */
  query: string
  /** Section copy. */
  t: (key: keyof typeof en) => string
  /** Adopt one row. */
  onPick: (row: ProviderRow) => void
  /** Record the search text. */
  onQueryChange: (query: string) => void
}

/**
 * Render the dormant provider picker inside the add modal.
 * @param props - rows, search text, and the pick action.
 * @returns the searchable provider grid.
 */
function CatalogPicker(props: CatalogPickerProps): ReactNode {
  const { rows, query, t, onPick, onQueryChange } = props
  const visible = rows.filter(row => rowMatches(row, query))
  return (
    <div className={styles['catalogPicker']}>
      <input
        className={styles['input']}
        type="search"
        value={query}
        placeholder={t('searchProviders')}
        aria-label={t('searchProviders')}
        onChange={(event) => { onQueryChange(event.target.value) }}
      />
      {visible.length === 0
        ? <p className={styles['emptyState']}>{t('noMatchingProviders')}</p>
        : (
          <ul className={styles['catalogList']} aria-label={t('addCatalog')}>
            {visible.map(row => (
              <li key={row.entry.provider}>
                <button
                  type="button"
                  className={styles['catalogChoice']}
                  aria-label={row.entry.provider}
                  onClick={() => { onPick(row) }}
                >
                  <span className={styles['catalogChoiceName']}>{row.entry.displayName}</span>
                  <span className={styles['catalogChoiceRoute']}>{row.entry.provider}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
    </div>
  )
}

/**
 * The copy a landed fetch shows, naming both counts the sync moved.
 * @param added - rows the fetch adopted.
 * @param removed - stale fetched rows the sync pruned.
 * @param t - section copy.
 * @returns the banner text without the provider prefix.
 */
function fetchOutcome(added: number, removed: number, t: (key: keyof typeof en) => string): string {
  if (removed === 0) return added === 0 ? t('fetchComplete') : t('fetchAdded').replace('{count}', () => String(added))
  if (added === 0) return t('fetchRemoved').replace('{removed}', () => String(removed))
  return t('fetchSynced')
    .replace('{count}', () => String(added))
    .replace('{removed}', () => String(removed))
}

/**
 * The banner a settled row write shows: the provider as the directory names it
 * now, and the copy that reports the outcome.
 * @param notice - the settled write.
 * @param rows - the rows the refreshed page lists.
 * @param t - section copy.
 * @returns the banner view.
 */
function shownNotice(notice: RowNotice, rows: readonly ProviderRow[], t: (key: keyof typeof en) => string): ShownNotice {
  // A save can rename the route, and a row the same write removed keeps the
  // identity captured when the control was used: nothing newer names it.
  const row = rows.find(candidate => candidate.entry.provider === notice.target.provider)
  const identity = row === undefined
    ? notice.target
    : { provider: row.entry.provider, displayName: row.entry.displayName }
  const outcome = notice.kind === 'saved'
    ? providerCopy(t('savedProvider'), identity)
    : notice.kind === 'fetched'
      ? notice.empty
        ? t('fetchEmpty')
        : providerCopy(fetchOutcome(notice.added, notice.removed, t), identity)
      : providerCopy(t(notice.stale ? 'writeStale' : 'writeFailed'), identity)
        .replace('{detail}', () => notice.detail)
  return {
    holdMs: notice.kind === 'failed' ? FAILURE_HOLD_MS : SUCCESS_HOLD_MS,
    identity,
    kind: notice.kind,
    retry: notice.retry,
    seq: notice.seq,
    text: outcome,
  }
}

/**
 * Remove one user-added provider and its page-managed credential. Credential
 * removal comes first so a second-step failure leaves the provider row visible
 * and the whole operation safely retryable; both unsets are idempotent.
 * The settings removal names the profile rather than rebuilding its whole
 * namespace from a partial view.
 * @param operations - the page's Host operations.
 * @param controller - the page store to refresh.
 * @param target - the provider's settings address and optional managed credential.
 * @returns the failure message, or undefined once the write and reload landed.
 */
export async function removeProviderProfile(
  operations: ModelsOperations,
  controller: ModelsSettingsStore,
  target: { settingsNs: string; settingsPath: readonly string[]; credentialRef?: string },
): Promise<string | undefined> {
  if (target.credentialRef !== undefined) {
    const credential = await operations.removeCredential(target.credentialRef)
    if (credential !== undefined) return credential
  }
  const written = await operations.writeSettings(
    target.settingsNs,
    [{ op: 'unset', path: [...target.settingsPath] }],
    undefined,
  )
  if (written.kind !== 'written') return written.message
  await controller.load()
  return undefined
}

/**
 * Whether a whole-section provider still needs its first key: an unconfigured
 * credential opens the setup card instead of showing a row. This is the
 * first-run posture alone — a user who can already reach some provider gets an
 * ordinary row with the attention dot, since nothing here is blocking them.
 * @param row - the joined provider row.
 * @param anyUsable - whether any joined row can already serve requests.
 * @returns whether the page opens this provider before the user chooses one.
 */
export function needsSetup(row: ProviderRow, anyUsable: boolean): boolean {
  if (anyUsable || row.entry.provider === 'deepseek-account') return false
  if (row.entry.settingsPath.length > 0) return false
  return row.credential?.configured !== true
}

/**
 * The provider-card seat's credential fact: the reference this page would use
 * for the row — the profile's `apiKeyEnv`, or the page's derived
 * `<ROUTE>_API_KEY` while the profile names none — confirmed configured. The
 * derived half is what keeps the seat consistent with the editor on the
 * add-provider draft, whose dormant row names no reference yet.
 */
function keyConfiguredOf(row: ProviderRow): boolean {
  return row.apiKeyEnv !== undefined
    ? row.credential?.configured === true
    : row.derivedCredential?.configured === true
}

/**
 * Render the Models section.
 * @param props - slot-delivered injected dependencies.
 * @returns the section, or null while the shell has not injected yet.
 */
export function ModelsSection(props: ModelsSectionProps): ReactNode {
  const { controller, useSnapshot, operations, schema, t, renderSlot } = props
  if (
    controller === undefined || useSnapshot === undefined || operations === undefined
    || schema === undefined || t === undefined
  ) return null
  return <Loaded injected={{ controller, useSnapshot, operations, schema, t }} renderSlot={renderSlot} />
}

function Loaded({ injected, renderSlot }: { injected: ModelsSectionFace; renderSlot: ModelsRenderSlot }): ReactNode {
  const { controller, operations, schema, t } = injected
  const snapshot = injected.useSnapshot(value => value)
  const state = { ...snapshot, rows: snapshot.rows.map(row => row.entry.provider === 'deepseek-account'
    ? { ...row, entry: { ...row.entry, displayName: t('deepSeekAccount') } } : row) }
  /** A new or failed surface waits for a current snapshot; background refreshes preserve its drafts. */
  const [hasReadySnapshot, setHasReadySnapshot] = useState(state.status === 'ready')
  if (state.status === 'ready' && !hasReadySnapshot) setHasReadySnapshot(true)
  if (state.status === 'error' && hasReadySnapshot) setHasReadySnapshot(false)
  /**
   * The provider row the user chose: `undefined` before any choice, so the
   * first-run posture still opens the provider that has no key anywhere, and
   * `null` once the editor has been closed.
   */
  const [picked, setPicked] = useState<string | null | undefined>(undefined)
  const [addOpen, setAddOpen] = useState(false)
  const [addMode, setAddMode] = useState<AddMode>('catalog')
  /** The modes shown since the add modal opened; each keeps its panel mounted. */
  const [visited, setVisited] = useState<ReadonlySet<AddMode>>(() => new Set())
  /** Whether each add panel has a write or an interrogation in flight. */
  const [catalogBusy, setCatalogBusy] = useState(false)
  const [customBusy, setCustomBusy] = useState(false)
  /** The catalog draft's row, kept through a refresh that adopts or withdraws it elsewhere. */
  const [draftTarget, setDraftTarget] = useState<EditorTarget | undefined>(undefined)
  /** Base of the add modal's tab and panel ids. */
  const addId = useId()
  /** Search text over the configured rows. */
  const [query, setQuery] = useState('')
  /** Search text over the add modal's dormant providers. */
  const [addQuery, setAddQuery] = useState('')
  const [deleteTarget, setDeleteTarget] = useState<EditorTarget | undefined>(undefined)
  const [deleting, setDeleting] = useState(false)
  const [deleteFailure, setDeleteFailure] = useState<string | undefined>(undefined)
  /** Routes with a row-level write in flight; their list rows show it. */
  const [pendingRows, setPendingRows] = useState<ReadonlySet<string>>(() => new Set())
  const [notice, setNotice] = useState<RowNotice | undefined>(undefined)
  const noticeSeq = useRef(0)

  /** Publish one settled write; a repeat of the same outcome restarts the banner. */
  const report = (next: RowNoticeInput): void => {
    noticeSeq.current += 1
    setNotice({ ...next, seq: noticeSeq.current })
  }

  /**
   * Apply one row-level write at the revision the listed rows were read at, then
   * publish its settled outcome. A conflict re-reads the page so the list shows
   * what the Host holds now and offers the write back to the user; the write is
   * never replayed on its own.
   * @param write - the namespace, operations, and row the control names.
   * @param outcome - what a landed write announces: a save, or a fetch's counts.
   */
  const runRowWrite = (
    write: RowWrite,
    outcome: { kind: 'saved' } | { kind: 'fetched'; added: number; removed: number } = { kind: 'saved' },
  ): void => {
    setPendingRows(current => new Set([...current, write.provider]))
    const revision = controller.store.getSnapshot().namespaces.get(write.ns)?.revision
    void operations.writeSettings(write.ns, write.ops, revision)
      .then(async (written) => {
        const target = { provider: write.provider, displayName: write.displayName }
        if (written.kind === 'written') {
          await controller.load()
          report({
            ...outcome.kind === 'fetched'
              ? { kind: 'fetched' as const, added: outcome.added, removed: outcome.removed, empty: false }
              : { kind: 'saved' as const },
            detail: '',
            retry: undefined,
            stale: false,
            target,
          })
          return
        }
        const stale = written.kind === 'conflict'
        if (stale) await controller.load()
        report({
          detail: written.message,
          kind: 'failed',
          retry: stale ? write : undefined,
          stale,
          target,
        })
      })
      .finally(() => {
        setPendingRows((current) => {
          const next = new Set(current)
          next.delete(write.provider)
          return next
        })
      })
  }

  /**
   * Ask one route what it serves and sync its list to the reply: the rows the
   * route already lists keep their tuned fields, every model nothing serves
   * yet joins, and a fetched row the endpoint no longer reports is removed. A
   * row the user added by hand is never removed. A route the adapter ships
   * keeps serving its installed catalog until the user layer replaces it, so
   * the reply adds nothing there and the write is skipped instead of
   * materializing a list that would change nothing while freezing the catalog.
   * @param item - the rail row whose fetch action was used.
   * @param probe - what that route's fetch asks, or why it cannot ask.
   */
  const runRowFetch = (item: RailItem, probe: FetchProbe): void => {
    /* v8 ignore next -- the action renders only for a route with something to ask */
    if (probe.kind !== 'ask') return
    const namespace = controller.store.getSnapshot().namespaces.get(item.target.settingsNs)
    /* v8 ignore next -- the action renders only for a configured row, whose namespace resolved */
    if (namespace === undefined) return
    setPendingRows(current => new Set([...current, item.provider]))
    void operations.discoverModels(item.target.settingsNs, {
      provider: probe.provider,
      ...probe.baseURL === undefined ? {} : { baseURL: probe.baseURL },
      ...probe.api === undefined ? {} : { api: probe.api },
    }).then((answer) => {
      if (answer.kind === 'refused') {
        report({ detail: answer.message, kind: 'failed', retry: undefined, stale: false, target: item.target })
        return
      }
      const explicit = modelDrafts(schema.getPath(namespace.value, [...item.target.settingsPath, 'models']))
      const served = item.target.declared !== true && explicit.length === 0
        ? answer.models.map(model => model.id)
        : servedModelIds(schema, namespace, item.target.settingsPath)
      const merged = mergeDiscoveredModels(served, explicit, answer.models)
      if (merged.added === 0 && merged.removed === 0) {
        report({
          added: 0, removed: 0, empty: false, detail: '', kind: 'fetched', retry: undefined, stale: false, target: item.target,
        })
        return
      }
      if (merged.models.length === 0) {
        // A route that serves no model cannot be resolved, so pruning every
        // row would break the route. The rows stay and the banner says the
        // endpoint listed nothing.
        report({
          added: 0, removed: 0, empty: true, detail: '', kind: 'fetched', retry: undefined, stale: false, target: item.target,
        })
        return
      }
      // An order naming a removed row would be refused by both adapters, so the
      // write carries the surviving ids.
      const storedOrder = schema.getPath(namespace.value, [...item.target.settingsPath, 'modelOrder'])
      const order = Array.isArray(storedOrder)
        ? storedOrder.filter((id): id is string => typeof id === 'string')
        : []
      runRowWrite(
        {
          provider: item.target.provider,
          ns: item.target.settingsNs,
          ops: [
            ...modelsArrayOps(item.target.settingsPath, merged.models),
            ...modelOrderSyncOps(item.target.settingsPath, order, merged.models),
          ],
          displayName: item.target.displayName,
        },
        { kind: 'fetched', added: merged.added, removed: merged.removed },
      )
    }).finally(() => {
      setPendingRows((current) => {
        const next = new Set(current)
        next.delete(item.provider)
        return next
      })
    })
  }

  /** Announce a card's save once the refreshed directory is in the snapshot. */
  const announceSaved = (target: ProviderIdentity): void => {
    // Announced only once the refreshed directory is in the snapshot the
    // notice reads its name from: an apply can rename the route, and the
    // target captured when the card opened still carries the old name.
    void controller.load().then(() => {
      report({ detail: '', kind: 'saved', retry: undefined, stale: false, target })
    })
  }

  /**
   * Close the add modal whole. The catalog target is forgotten with it, since a
   * refresh could otherwise leave a draft open for a provider the user never
   * chose. The busy flags reset here because a panel that closes itself on
   * success unmounts before it can report idle.
   */
  const closeAdd = (): void => {
    setDraftTarget(undefined)
    setAddQuery('')
    setAddOpen(false)
    setCatalogBusy(false)
    setCustomBusy(false)
  }

  /** Close the selected row's card once its save refresh settles. */
  const closeEditor = (changed: boolean, target: ProviderIdentity): void => {
    if (!changed) {
      setPicked(null)
      return
    }
    // Keep the card mounted while the refreshed snapshot is loading so a row
    // action cannot race the editor it just closed; then collapse and report.
    void controller.load().then(() => {
      setPicked(null)
      report({ detail: '', kind: 'saved', retry: undefined, stale: false, target })
    })
  }

  /** Close the add modal's catalog draft, which owns the draft target with it. */
  const closeDraft = (changed: boolean, target: ProviderIdentity): void => {
    closeAdd()
    if (changed) announceSaved(target)
  }

  const closeDelete = (): void => {
    if (deleting) return
    setDeleteTarget(undefined)
    setDeleteFailure(undefined)
  }

  const confirmDelete = (): void => {
    /* v8 ignore next -- the action only renders with a target and is disabled while a deletion is pending */
    if (deleteTarget === undefined || deleting) return
    setDeleting(true)
    setDeleteFailure(undefined)
    void removeProviderProfile(operations, controller, deleteTarget)
      .then((failure) => {
        if (failure !== undefined) {
          setDeleteFailure(failure)
          return
        }
        setDeleteTarget(undefined)
      })
      .finally(() => { setDeleting(false) })
  }

  if (state.status === 'idle') void controller.load()
  if (state.status === 'error') {
    /* v8 ignore next -- an error status always carries text; the fallback satisfies the nullable type */
    const errorText = state.error ?? ''
    return (
      <div className={styles['section']}>
        <p className={styles['error']}>{`${t('loadFailed')}: ${errorText}`}</p>
        <button type="button" className={styles['secondaryButton']} onClick={() => { void controller.load() }}>
          {t('retry')}
        </button>
      </div>
    )
  }
  if (!hasReadySnapshot && state.status !== 'ready') {
    return (
      <div className={styles['section']}>
        <h2 className={styles['title']}>{t('title')}</h2>
        <p className={styles['intro']}>{t('intro')}</p>
      </div>
    )
  }

  // One fact decides both first-run postures on this page and the onboarding
  // step: whether the user already has a provider to talk to.
  const anyUsable = state.rows.some(providerUsable)
  const configurable = state.rows.filter(row => state.namespaces.has(row.entry.settingsNs))
  const addable: AddableRow[] = state.rows.flatMap((row) => {
    const namespace = state.namespaces.get(row.entry.settingsNs)
    return namespace === undefined || row.configured ? [] : [{ row, namespace }]
  })
  const configured: ConfiguredRow[] = state.rows
    .filter(row => row.configured)
    .flatMap((row) => {
      const namespace = state.namespaces.get(row.entry.settingsNs)
      // The join marks a row configured only while its namespace resolved.
      /* v8 ignore next 1 -- unreachable while `configured` is the join's own flag */
      if (namespace === undefined) return []
      const target = targetOf(row)
      return [{ row, namespace, target, modelIds: servedModelIds(schema, namespace, target.settingsPath) }]
    })
  // The row the page opens before the user chooses one: the first-run provider
  // with no configured key anywhere.
  const opened = configured.find(candidate => needsSetup(candidate.row, anyUsable))
  const selectedId = picked === undefined ? opened?.row.entry.provider : picked === null ? undefined : picked
  const selected = selectedId === undefined
    ? undefined
    : configured.find(candidate => candidate.row.entry.provider === selectedId)
  // Positions count against every reorderable row the page lists, not the
  // filtered ones, so hiding rows cannot reorder what the user searched away.
  const order = movableProviderIds(state.rows)
  const items: RailItem[] = configured
    .map(candidate => candidate.row)
    .filter(row => rowMatches(row, query))
    .map(row => railItem(row, {
      selected: selectedId === row.entry.provider,
      pending: pendingRows.has(row.entry.provider),
      ready: state.status === 'ready',
      writable: state.writable,
      fetch: fetchProbe(schema, row, state.namespaces.get(row.entry.settingsNs)),
      t,
    }))
  // Hand-declared routes live in the pi-ai namespace, which is also the only
  // one whose schema names the protocols one may speak; without it mounted
  // there is nothing to declare and the mode is not offered.
  const piAi = state.namespaces.get('llm-pi-ai')
  const protocols = protocolChoices(piAi, schema)
  const reasoningLevels = reasoningLevelChoices(piAi, schema)
  // Each mode is offered while its namespace is mounted and enabled while it
  // has something to offer; the card shows the chosen mode where both are
  // offered, else the only one there is. A mode's panel is mounted while it is
  // the shown mode or has been shown since the card opened — derived, so a
  // refresh that changes which modes are offered can never leave the card
  // without a panel.
  const catalogOffered = configurable.length > 0
  const catalogEnabled = addable.length > 0
  const customOffered = piAi !== undefined
  const customEnabled = protocols.length > 0
  const addOffered = catalogOffered || customOffered
  const bothOffered = catalogOffered && customOffered
  const mode: AddMode = bothOffered ? addMode : customOffered ? 'custom' : 'catalog'
  const mounted = (candidate: AddMode): boolean => mode === candidate || visited.has(candidate)
  const switchLocked = catalogBusy || customBusy
  const keptNamespace = draftTarget === undefined ? undefined : state.namespaces.get(draftTarget.settingsNs)
  // The draft the catalog mode edits: the row the user chose, kept through a
  // refresh that adopts or withdraws it elsewhere so a typed key is never
  // discarded, for as long as its namespace can still take the write. No target
  // means the picker is still showing; no automatic first row.
  const draft: CatalogDraft | undefined = !addOpen || !catalogOffered
    || draftTarget === undefined || keptNamespace === undefined
    ? undefined
    : { target: draftTarget, namespace: keptNamespace }
  // The draft's directory row, for the card extension seat. A refresh can drop
  // the row mid-draft (the route was adopted or withdrawn elsewhere); the draft
  // card stays while the seat simply has no row to dispatch.
  const addRow = draft === undefined
    ? undefined
    : state.rows.find(row => row.entry.provider === draft.target.provider)

  /** Make one configured row the list's expanded subject. */
  const selectRow = (item: RailItem): void => {
    const selected = selectedId === item.provider
    if (!selected && controller.store.getSnapshot().status !== 'ready') return
    setNotice(undefined)
    closeAdd()
    setPicked(selected ? null : item.provider)
  }

  /** Adopt one dormant directory row inside the add modal. */
  const pickCatalogRow = (row: ProviderRow): void => {
    setDraftTarget(targetOf(row))
    setAddMode('catalog')
    setVisited(current => new Set([...current, 'catalog']))
  }

  /** Pause one route, or bring a paused route back. */
  const toggleRow = (item: RailItem): void => {
    setNotice(undefined)
    runRowWrite(pauseWrite(item.target, !item.enabled))
  }

  /** Run one of a row's glyph actions: fetch its list, edit it, or delete it. */
  const runRowAction = (item: RailItem, action: RowAction): void => {
    if (action.id === 'fetch') {
      const row = state.rows.find(candidate => candidate.entry.provider === item.provider)
      /* v8 ignore next -- the action renders only for a row the page lists */
      if (row === undefined) return
      const probe = fetchProbe(schema, row, state.namespaces.get(row.entry.settingsNs))
      /* v8 ignore next -- the action renders only while a probe exists */
      if (probe === undefined) return
      setNotice(undefined)
      runRowFetch(item, probe)
      return
    }
    if (action.id === 'edit') {
      // The edit action is the row's own select: it opens the card the name
      // does, and closes it again on a second use.
      selectRow(item)
      return
    }
    if (controller.store.getSnapshot().status !== 'ready') return
    setNotice(undefined)
    setDeleteFailure(undefined)
    setDeleteTarget(item.target)
  }

  /** Store the list's new provider row order. */
  const moveRow = (item: RailItem, to: number): void => {
    setNotice(undefined)
    runRowWrite(providerOrderWrite(order, item, to))
  }

  const openAdd = (): void => {
    if (controller.store.getSnapshot().status !== 'ready') return
    const initial: AddMode = catalogEnabled ? 'catalog' : 'custom'
    setNotice(undefined)
    setDraftTarget(undefined)
    setAddQuery('')
    setAddMode(initial)
    setVisited(new Set([initial]))
    setAddOpen(true)
  }

  const shown = notice === undefined ? undefined : shownNotice(notice, state.rows, t)
  const retry = shown === undefined ? undefined : shown.retry
  return (
    <div className={styles['section']}>
      <div className={styles['sectionHead']}>
        <div className={styles['sectionTitleRow']}>
          <div className={styles['sectionHeading']}>
            <h2 className={styles['title']}>{t('title')}</h2>
            <p className={styles['intro']}>{t('intro')}</p>
          </div>
          {addOffered
            ? (
              <button
                type="button"
                className={styles['primaryButton']}
                aria-label={t('add')}
                disabled={!state.writable || state.status !== 'ready' || (!catalogEnabled && !customEnabled)}
                onClick={openAdd}
              >
                <IconPlusOutlineRegular size={14} />
                <span>{t('add')}</span>
              </button>
            )
            : null}
        </div>
        {!state.writable && state.status === 'ready' ? <p className={styles['notice']}>{t('readOnly')}</p> : null}
      </div>
      {configured.length === 0
        ? <p className={styles['detailEmpty']}>{t('emptyProviders')}</p>
        : (
          <div className={styles['toolbar']}>
            <input
              className={`${styles['input']} ${styles['toolbarSearch']}`}
              type="search"
              value={query}
              placeholder={t('searchProviders')}
              aria-label={t('searchProviders')}
              onChange={(event) => { setQuery(event.target.value) }}
            />
          </div>
        )}
      {configured.length > 0
        ? (
          <ReorderList
            items={items}
            order={order}
            t={t}
            onToggle={toggleRow}
            onRowAction={runRowAction}
            onMove={moveRow}
            onSelect={selectRow}
            renderDetail={(item) => {
              if (selected === undefined || selected.row.entry.provider !== item.provider) return null
              const current = selected
              return (
                <div className={styles['detailCard']}>
                  <div className={styles['rowHead']}>
                    <span className={styles['rowIdentity']}>
                      <span className={styles['rowName']}>{current.row.entry.displayName}</span>
                      {current.row.entry.provider === current.row.entry.displayName
                        ? null
                        : <span className={styles['editorRoute']}>{current.row.entry.provider}</span>}
                      {/* The store keeps the account row only while the account lists models. */}
                      {current.row.entry.provider === 'deepseek-account'
                        ? (
                          <span className={styles['accountStatus']}>
                            <span className={`${styles['credentialDot']} ${styles['credentialDotConfigured']}`} aria-hidden="true" />
                            {t('accountSignedIn')}
                          </span>
                        )
                        : null}
                    </span>
                    <span className={styles['rowActions']}>
                      <button
                        type="button"
                        className={styles['secondaryButton']}
                        onClick={() => { setNotice(undefined); setPicked(null) }}
                      >
                        {t('close')}
                      </button>
                    </span>
                  </div>
                  {current.row.entry.error === undefined
                    ? null
                    : <p role="alert" className={styles['error']}>{current.row.entry.error}</p>}
                  {renderSlot(
                    'settings.models.provider-card',
                    {
                      provider: current.row.entry,
                      configured: current.row.configured,
                      keyConfigured: keyConfiguredOf(current.row),
                    },
                    { entryKey: current.row.entry.settingsNs },
                  )}
                  {/* The account catalog comes from the account listing; its model
                      overrides live in cordis.patch.yml. */}
                  {current.row.entry.provider === 'deepseek-account'
                    ? null
                    : renderProviderEditor({
                      target: current.target,
                      row: current.row,
                      namespace: current.namespace,
                      schema,
                      operations,
                      t,
                      readOnly: !state.writable,
                      onClose: (changed) => { closeEditor(changed, current.target) },
                    })}
                  {current.modelIds.length > 1
                    ? (
                      <ModelOrderCard
                        ids={current.modelIds}
                        pending={pendingRows.has(current.row.entry.provider)}
                        readOnly={!state.writable}
                        t={t}
                        onMove={(id, to) => {
                          setNotice(undefined)
                          runRowWrite(modelOrderWrite(current.target, current.modelIds, id, to))
                        }}
                      />
                    )
                    : null}
                </div>
              )
            }}
          />
        )
        : null}
      {configured.length > 0 && items.length === 0
        ? <p className={styles['emptyState']}>{t('noMatchingProviders')}</p>
        : null}
      {configured.length > 0 && selected === undefined && items.length > 0
        ? <p className={styles['detailEmpty']}>{t('detailEmpty')}</p>
        : null}
      {renderSlot('settings.models.footer', {})}
      {shown === undefined
        ? null
        : (
          <Toast
            key={`models-notice-${String(shown.seq)}`}
            text={shown.text}
            holdMs={shown.holdMs}
            {...shown.kind === 'failed'
              ? { icon: <IconWarningOutlineRegular /> }
              : { politeness: 'polite' as const, tone: 'success' as const }}
            {...retry === undefined ? {} : { actions: [{ label: t('retry'), onClick: () => { runRowWrite(retry) } }] }}
            onDone={() => { setNotice(undefined) }}
          />
        )}
      <Modal
        open={addOpen}
        onClose={closeAdd}
        title={t('add')}
        closeLabel={t('close')}
        className={styles['addDialog'] as string}
        contentClassName={styles['addDialogContent'] as string}
      >
        <div className={styles['addModes']}>
          {bothOffered
            ? (
              <SegmentedControl
                id={addId}
                label={t('addMode')}
                value={mode}
                disabled={switchLocked}
                options={[
                  {
                    value: 'catalog',
                    label: t('addCatalog'),
                    disabled: !catalogEnabled,
                    ...catalogEnabled ? {} : { title: t('addCatalogExhausted') },
                  },
                  {
                    value: 'custom',
                    label: t('addCustom'),
                    disabled: !customEnabled,
                    ...customEnabled ? {} : { title: t('addCustomUnavailable') },
                  },
                ]}
                onChange={(next) => {
                  setAddMode(next)
                  setVisited(current => new Set([...current, next]))
                }}
              />
            )
            : (
              <div className={styles['editorHeader']}>
                <span className={styles['editorTitle']}>{t(mode === 'catalog' ? 'addCatalog' : 'addCustom')}</span>
              </div>
            )}
          <p className={styles['advancedHint']}>
            {t(mode === 'catalog' ? 'addCatalogHint' : 'addCustomHint')}
          </p>
        </div>
        {mounted('catalog')
          ? (
            <div
              id={`${addId}-catalog-panel`}
              {...bothOffered ? { role: 'tabpanel', 'aria-labelledby': `${addId}-catalog` } : {}}
              hidden={mode !== 'catalog'}
              className={styles['addPanel']}
            >
              {draft === undefined
                ? (
                  <CatalogPicker
                    rows={addable.map(candidate => candidate.row)}
                    query={addQuery}
                    t={t}
                    onPick={pickCatalogRow}
                    onQueryChange={setAddQuery}
                  />
                )
                : (
                  <>
                    <div className={styles['draftHead']}>
                      <button
                        type="button"
                        className={styles['linkButton']}
                        onClick={() => { setDraftTarget(undefined) }}
                      >
                        {t('backToProviders')}
                      </button>
                      <span className={styles['editorTitle']}>{draft.target.displayName}</span>
                    </div>
                    <ProviderEditor
                      key={draft.target.provider}
                      provider={draft.target.provider}
                      displayName={draft.target.displayName}
                      hideTitle
                      namespace={draft.namespace}
                      schema={schema}
                      settingsPath={draft.target.settingsPath}
                      operations={operations}
                      {...draft.target.defaultBaseURL === undefined ? {} : { defaultBaseURL: draft.target.defaultBaseURL }}
                      {...draft.target.baseURLEnv === undefined ? {} : { baseURLEnv: draft.target.baseURLEnv }}
                      {...draft.target.nativeAuth === undefined ? {} : { nativeAuth: draft.target.nativeAuth }}
                      {...addRow?.credentialChecks === undefined ? {} : { credentialChecks: addRow.credentialChecks }}
                      t={t}
                      readOnly={!state.writable}
                      onClose={(changed) => { closeDraft(changed, draft.target) }}
                      onBusyChange={setCatalogBusy}
                    />
                    {addRow === undefined
                      ? null
                      : renderSlot(
                        'settings.models.provider-card',
                        { provider: addRow.entry, configured: addRow.configured, keyConfigured: keyConfiguredOf(addRow) },
                        { entryKey: addRow.entry.settingsNs },
                      )}
                  </>
                )}
            </div>
          )
          : null}
        {mounted('custom') && piAi !== undefined
          ? (
            <div
              id={`${addId}-custom-panel`}
              {...bothOffered ? { role: 'tabpanel', 'aria-labelledby': `${addId}-custom` } : {}}
              hidden={mode !== 'custom'}
              className={styles['addPanel']}
            >
              <CustomProviderCard
                taken={state.rows.map(row => row.entry.provider)}
                protocols={protocols}
                reasoningLevels={reasoningLevels}
                revision={piAi.revision}
                operations={operations}
                t={t}
                readOnly={!state.writable}
                onClose={(changed) => {
                  closeAdd()
                  if (changed) void controller.load()
                }}
                onBusyChange={setCustomBusy}
              />
            </div>
          )
          : null}
      </Modal>
      <Modal
        open={deleteTarget !== undefined}
        onClose={closeDelete}
        title={deleteTarget === undefined ? '' : providerCopy(t('deleteTitle'), deleteTarget)}
        closeLabel={t('close')}
        description={deleteTarget === undefined
          ? ''
          : providerCopy(
            deleteTarget.credentialRef === undefined
              ? t('deleteDescription')
              : t('deleteDescriptionWithCredential'),
            deleteTarget,
          )}
        className={styles['deleteDialog'] as string}
        footer={(
          <>
            <Button variant="outline" data-modal-autofocus disabled={deleting} onClick={closeDelete}>
              {t('cancel')}
            </Button>
            <Button
              variant="outline"
              className={styles['deleteConfirm']}
              disabled={deleting}
              onClick={confirmDelete}
            >
              {deleteTarget === undefined
                ? ''
                : providerCopy(deleting ? t('deleting') : t('deleteConfirm'), deleteTarget)}
            </Button>
          </>
        )}
      >
        {deleteFailure === undefined ? null : <p className={styles['error']}>{deleteFailure}</p>}
      </Modal>
    </div>
  )
}
