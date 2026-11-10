/**
 * The row-level view model behind the Models page's provider list and reorder
 * controls: the provider an editor or a write addresses, what one row states and
 * offers, which rows a search term names, which routes a stored `providerOrder`
 * may list, the order one route serves its models in, and the settings write one
 * row control performs.
 *
 * Every fact is derived from the joined page snapshot and the owning namespace's
 * schema or resolved value, so a row cannot state something the Host does not
 * carry. Copy arrives through the caller's `t` seat.
 */

import type { LlmNativeAuth, SettingsNamespaceView, SettingsPathOpView } from '@deepseek-ai/dsh-api-remotes/client'
import { modelOrderOps, moveItem, providerEnabledOps, providerOrderOps } from './mutations.ts'
import type { ProviderRow, ProviderRowStatus } from './store.ts'
import { deriveKeyRef, providerRowStatus } from './store.ts'
import type { ModelsKey } from './locales.ts'
import type { ReorderItem, RowAction } from './ReorderList.tsx'
import type { StateDotState } from '@deepseek-ai/dsh-client-ui-primitives'
import type { SettingsSchemaOperations } from './schema-operations.ts'

/** Provider identity shared by row actions, outcome notices, and confirmation copy. */
export interface ProviderIdentity {
  /** Stable provider route id. */
  provider: string
  /** Human-facing provider name. */
  displayName: string
}

/** One existing row or dormant directory entry addressed by an editor action. */
export interface EditorTarget extends ProviderIdentity {
  settingsNs: string
  settingsPath: readonly string[]
  /** Writable credential identified under this page's conventional reference. */
  credentialRef?: string
  /** The adapter reports this route as one it does not ship (see `ProviderEditor.declared`). */
  declared?: boolean
  /** The route's catalog endpoint (see `ProviderEditor.defaultBaseURL`). */
  defaultBaseURL?: string
  /** Environment used to construct an endpoint (see `ProviderEditor.baseURLEnv`). */
  baseURLEnv?: readonly string[]
  /** The route's native authentication (see `ProviderEditor.nativeAuth`). */
  nativeAuth?: LlmNativeAuth
}

/**
 * The settings address and provider-owned fields of one joined row.
 * @param row - the joined row.
 * @returns the target an editor or a row-level write is opened against.
 */
export function targetOf(row: ProviderRow): EditorTarget {
  const managedRef = deriveKeyRef(row.entry.provider)
  const credentialRef = row.apiKeyEnv === managedRef
    && row.credential?.configured === true
    && row.credential.writable
    ? managedRef
    : undefined
  return {
    provider: row.entry.provider,
    displayName: row.entry.displayName,
    settingsNs: row.entry.settingsNs,
    settingsPath: row.entry.settingsPath,
    ...credentialRef === undefined ? {} : { credentialRef },
    // Only declared routes may expose route-owned fields.
    ...row.entry.declared === true ? { declared: true } : {},
    ...row.entry.defaultBaseURL === undefined ? {} : { defaultBaseURL: row.entry.defaultBaseURL },
    ...row.entry.baseURLEnv === undefined ? {} : { baseURLEnv: row.entry.baseURLEnv },
    ...row.entry.nativeAuth === undefined ? {} : { nativeAuth: row.entry.nativeAuth },
  }
}

/**
 * Stable visible and accessible name for one provider.
 * @param target - the route id and the name the directory gives it.
 * @returns the name, with the route id beside it whenever the two differ.
 */
export function providerTargetLabel(target: ProviderIdentity): string {
  return target.provider === target.displayName
    ? target.provider
    : `${target.displayName} (${target.provider})`
}

/**
 * Replace the one provider placeholder in localized row and confirmation copy.
 * @param template - a dictionary string carrying `{provider}`.
 * @param target - the provider the copy names.
 * @returns the string to show.
 */
export function providerCopy(template: string, target: ProviderIdentity): string {
  return template.replace('{provider}', () => providerTargetLabel(target))
}

/**
 * The dot a provider row shows for its status. `done` is a route usable now;
 * `warning` a route the user has added but cannot yet reach, so no request can
 * get to it; `error` a configuration the adapter refused; `idle` a route waiting
 * on nothing — either paused, which one toggle ends, or a catalog offer the user
 * has never added.
 */
const RAIL_DOT: Record<ProviderRowStatus, StateDotState> = {
  ready: 'done',
  attention: 'warning',
  error: 'error',
  paused: 'idle',
  unadopted: 'idle',
}

/** The status name a rail row states beside its dot. */
const RAIL_STATUS: Record<ProviderRowStatus, ModelsKey> = {
  ready: 'statusReady',
  attention: 'statusAttention',
  error: 'statusError',
  paused: 'statusPaused',
  unadopted: 'statusUnadopted',
}

/**
 * Whether the page may list one route at another position. Only `llm-pi-ai`
 * profiles carry the sortable group — the section refuses any key with no
 * profile of its own — and the account and official DeepSeek rows are pinned by
 * the harness rather than ordered.
 * @param row - the joined provider row.
 * @returns whether `providerOrder` may name the row.
 */
function sortableRow(row: ProviderRow): boolean {
  return row.entry.settingsNs === 'llm-pi-ai' && row.configured
}

/** One provider list row: the labels, state, and settings address it carries. */
export interface RailItem extends ReorderItem {
  /** Whether any layer configures the route; a dormant row is adopted instead. */
  readonly configured: boolean
  /** Whether the route registers; the pause toggle writes its opposite. */
  readonly enabled: boolean
  readonly provider: string
  readonly target: EditorTarget
}

/**
 * What a row-level fetch would ask, or why it cannot ask: the endpoint facts
 * the interrogation takes. `blocked` names the copy key explaining a route
 * that must name an endpoint before anything can be asked of it.
 */
export type FetchProbe =
  | { readonly kind: 'ask'; readonly provider: string; readonly baseURL?: string; readonly api?: string }
  | { readonly kind: 'blocked'; readonly reason: ModelsKey }

/** Context one row is projected in. */
export interface RailItemContext {
  /** Whether the page lists current rows; a refresh holds the controls still. */
  readonly ready: boolean
  /** Whether the row is the one the detail pane shows. */
  readonly selected: boolean
  /** Whether a row-level write for this route is in flight. */
  readonly pending: boolean
  /** Whether the settings document accepts writes. */
  readonly writable: boolean
  /** What this route's fetch action would ask; absent when it has none. */
  readonly fetch?: FetchProbe | undefined
  /** Section copy. */
  readonly t: (key: ModelsKey) => string
}

/**
 * The endpoint facts a row-level fetch would ask, from the route's resolved
 * profile. Only the pi-ai namespace registers model discovery, so no other
 * family offers the action; a route the adapter does not ship has nothing to
 * answer from but its own endpoint, which its profile must therefore name.
 * @param schema - settings schema and immutable path operations.
 * @param row - the joined provider row, which the page lists only while it is configured.
 * @param namespace - the namespace owning the route's profile, absent while unmounted.
 * @returns what the action would ask, or absent when the route cannot be interrogated.
 */
export function fetchProbe(
  schema: SettingsSchemaOperations,
  row: ProviderRow,
  namespace: SettingsNamespaceView | undefined,
): FetchProbe | undefined {
  if (namespace === undefined || row.entry.settingsNs !== 'llm-pi-ai') return undefined
  const at = (key: string): string | undefined => {
    const value = schema.getPath(namespace.value, [...row.entry.settingsPath, key])
    return typeof value === 'string' && value.length > 0 ? value : undefined
  }
  const baseURL = at('baseURL')
  if (row.entry.declared === true && baseURL === undefined) return { kind: 'blocked', reason: 'fetchNeedsBaseUrl' }
  const api = at('api')
  return {
    kind: 'ask',
    provider: row.entry.provider,
    ...baseURL === undefined ? {} : { baseURL },
    ...api === undefined ? {} : { api },
  }
}

/**
 * Project one joined provider row into its rail presentation.
 * @param row - the joined row.
 * @param context - selection, writability, in-flight state, and copy.
 * @returns the rail row.
 */
export function railItem(row: ProviderRow, context: RailItemContext): RailItem {
  const { displayName, provider, settingsNs } = row.entry
  const target = targetOf(row)
  const status = providerRowStatus(row)
  const held = !context.ready || !context.writable || context.pending
  // The account route's models come from the account listing, and the join
  // always reports it enabled, so no profile field of it can be paused here.
  const pauseable = settingsNs !== '' && row.configured && provider !== 'deepseek-account'
  const credential = row.apiKeyEnv !== undefined ? row.credential : row.derivedCredential
  const credentialState = provider === 'deepseek-account'
    ? undefined
    : credential?.configured === true
      ? 'configured' as const
      : row.apiKeyEnv !== undefined && credential?.configured === false
        ? 'missing' as const
        : undefined
  const editable = settingsNs !== ''
  const actions: RowAction[] = []
  const probe = context.fetch
  if (probe !== undefined) {
    actions.push({
      id: 'fetch',
      label: providerCopy(context.t('fetchProvider'), target),
      ...probe.kind === 'blocked'
        ? { blocked: context.t(probe.reason) }
        : { title: context.t('fetchOffered') },
      disabled: held || probe.kind === 'blocked',
    })
  }
  if (editable) {
    actions.push({ id: 'edit', label: providerCopy(context.t('editProvider'), target), disabled: context.pending })
  }
  if (row.removable) {
    actions.push({ id: 'delete', label: providerCopy(context.t('removeProvider'), target), disabled: held, danger: true })
  }
  return {
    id: provider,
    provider,
    target,
    configured: row.configured,
    enabled: row.enabled,
    label: displayName,
    dot: context.pending ? 'ongoing' : RAIL_DOT[status],
    // The account row states its own fact; every other row states its status.
    statusText: provider === 'deepseek-account'
      ? context.t('accountSignedIn')
      : context.t(RAIL_STATUS[status]),
    caption: row.entry.declared === true ? context.t('customTag') : undefined,
    ...credentialState === undefined ? {} : { credential: credentialState },
    selected: context.selected,
    movable: sortableRow(row),
    selectLabel: editable ? providerCopy(context.t('expandProvider'), target) : undefined,
    ...pauseable
      ? {
        toggle: {
          label: providerCopy(context.t('providerEnabled'), target),
          title: providerCopy(context.t(row.enabled ? 'pauseProvider' : 'resumeProvider'), target),
          checked: row.enabled,
          disabled: held,
        },
      }
      : {},
    actions,
    disabled: context.pending,
    held,
  }
}

/**
 * Whether one search term names a row: its product display name or its route id.
 * @param row - the joined provider row.
 * @param query - the search text as typed.
 * @returns whether the rail lists the row.
 */
export function rowMatches(row: ProviderRow, query: string): boolean {
  const needle = query.trim().toLowerCase()
  if (needle.length === 0) return true
  return row.entry.displayName.toLowerCase().includes(needle)
    || row.entry.provider.toLowerCase().includes(needle)
}

/**
 * The route keys a stored `providerOrder` may name: configured `llm-pi-ai`
 * routes, in the order the page lists them. The section refuses any key with no
 * profile of its own, and the account and official DeepSeek rows are pinned by
 * the harness rather than ordered.
 * @param rows - the rows the page lists.
 * @returns the reorderable route keys.
 */
export function movableProviderIds(rows: readonly ProviderRow[]): string[] {
  return rows
    .filter(row => row.entry.settingsNs === 'llm-pi-ai' && row.configured)
    .map(row => row.entry.provider)
}

/** One row-level settings write and the row it belongs to. */
export interface RowWrite {
  /** Route whose rail row shows the write in flight. */
  readonly provider: string
  /** Settings namespace owning the section field or profile. */
  readonly ns: string
  /** Path operations against the stored section. */
  readonly ops: SettingsPathOpView[]
  /** Provider name the outcome notice carries. */
  readonly displayName: string
}

/**
 * The write that stores a new provider row order.
 * @param order - the current reorderable route keys.
 * @param moved - the rail row the user moved.
 * @param to - the position it lands at within `order`.
 * @returns the row write against the `llm-pi-ai` section field.
 */
export function providerOrderWrite(order: readonly string[], moved: RailItem, to: number): RowWrite {
  return {
    provider: moved.provider,
    ns: 'llm-pi-ai',
    ops: providerOrderOps(moveItem(order, order.indexOf(moved.provider), to)),
    displayName: moved.label,
  }
}

/**
 * The write that pauses one route or brings it back.
 * @param target - the route's settings address.
 * @param enabled - the state the user asked for.
 * @returns the row write.
 */
export function pauseWrite(target: EditorTarget, enabled: boolean): RowWrite {
  return {
    provider: target.provider,
    ns: target.settingsNs,
    ops: providerEnabledOps(target.settingsPath, enabled),
    displayName: target.displayName,
  }
}

/**
 * The write that stores the order one route serves its models in.
 * @param target - the route's settings address.
 * @param order - the model ids in their current listed order.
 * @param moved - the model id the card moved.
 * @param to - the position it lands at within `order`.
 * @returns the row write.
 */
export function modelOrderWrite(
  target: EditorTarget,
  order: readonly string[],
  moved: string,
  to: number,
): RowWrite {
  return {
    provider: target.provider,
    ns: target.settingsNs,
    ops: modelOrderOps(target.settingsPath, moveItem(order, order.indexOf(moved), to)),
    displayName: target.displayName,
  }
}

/**
 * The model ids one route serves, in the order it serves them. A stored
 * `modelOrder` leads and names only served models; every model it leaves out
 * follows in the order the route's own list gives, so an order written before a
 * model was added never hides it.
 * @param schema - settings schema and immutable path operations.
 * @param namespace - the namespace owning the route's profile.
 * @param path - path of the route's profile inside that namespace.
 * @returns the served model ids, empty when the route resolves no model list.
 */
export function servedModelIds(
  schema: SettingsSchemaOperations,
  namespace: SettingsNamespaceView,
  path: readonly string[],
): string[] {
  const listed = schema.getPath(namespace.value, [...path, 'models'])
  const ids = Array.isArray(listed)
    ? listed.flatMap((item) => {
      const id = typeof item === 'object' && item !== null ? (item as { id?: unknown }).id : undefined
      return typeof id === 'string' && id.length > 0 ? [id] : []
    })
    : []
  const stored = schema.getPath(namespace.value, [...path, 'modelOrder'])
  const order = Array.isArray(stored)
    ? stored.filter((value): value is string => typeof value === 'string')
    : []
  const served = new Set(ids)
  return [...order.filter(id => served.has(id)), ...ids.filter(id => !order.includes(id))]
}
