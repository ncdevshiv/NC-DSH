/**
 * The settings path operations behind the Models page's row-level controls,
 * together with the value builders those writes carry.
 *
 * A row toggle writes one path rather than a rebuilt subtree: an open editor
 * card may hold unsaved edits to other fields of the same profile, and a
 * whole-subtree write would silently drop them. Returning a field to its
 * default unsets it instead of storing the default, so the user layer keeps
 * only the choices somebody actually made and a row can still be removed.
 */

import type { JsonValue } from '@deepseek-ai/dsh-util-values'
import type { LlmDiscoveredModel, SettingsPathOpView } from '@deepseek-ai/dsh-api-remotes/client'
import type { DeepSeekModelDraft } from './DeepSeekModelsEditor.tsx'

/**
 * Provenance of one served row, so a fetch can tell what it may prune from what
 * a person added.
 *
 * - `custom`: added by the user through Add model. A fetch never removes it.
 * - `discovered`: adopted by a fetch from the endpoint listing. A later fetch
 *   that no longer reports it removes it.
 * - absent: an entry written before this page recorded provenance, or written
 *   outside it. A fetch treats it as endpoint-owned, because the route's list is
 *   what the endpoint reported until somebody saves a row as custom.
 */
export type ModelSource = 'discovered' | 'custom'

/** Field carrying {@link ModelSource} on a stored model row. */
export const MODEL_SOURCE_FIELD = 'source'

/**
 * Adopt one model the provider disclosed as a served row, keeping only the
 * fields the reply carries so a row never declares a capacity nobody measured.
 * @param candidate - one model from the provider's listing.
 * @returns the row to store, marked discovered so a later fetch can prune it.
 */
export function adoptDiscoveredModel(candidate: LlmDiscoveredModel): DeepSeekModelDraft {
  return {
    id: candidate.id,
    ...candidate.name === undefined ? {} : { name: candidate.name },
    ...candidate.contextWindow === undefined ? {} : { contextWindow: candidate.contextWindow },
    ...candidate.maxTokens === undefined ? {} : { maxTokens: candidate.maxTokens },
    ...candidate.inputModalities === undefined ? {} : { input: [...candidate.inputModalities] },
    [MODEL_SOURCE_FIELD]: 'discovered' satisfies ModelSource,
  }
}

/**
 * What one fetch writes: every row the user layer owns, with a fetched row the
 * endpoint no longer reports removed and a row the user added by hand kept.
 * A row already served is never rewritten, so a fetch cannot overwrite a
 * capacity somebody corrected; the only rows it drops are ones a previous
 * fetch adopted for the same endpoint and this reply no longer names.
 *
 * A catalog route keeps serving its installed catalog from the layer beneath,
 * so its rows are inherited rather than written here; the fetch only prunes
 * adopted rows on a route whose list the user layer owns.
 * @param servedIds - ids the route already serves, from any layer.
 * @param explicit - the rows the user layer owns, in their stored order.
 * @param discovered - the models the provider reported, in its own order.
 * @returns the stored row array, how many rows it gained, and how many it removed.
 */
export function mergeDiscoveredModels(
  servedIds: readonly string[],
  explicit: readonly DeepSeekModelDraft[],
  discovered: readonly LlmDiscoveredModel[],
): { models: DeepSeekModelDraft[]; added: number; removed: number } {
  const served = new Set(servedIds)
  const reported = new Set(discovered.map(model => model.id))
  // Only a row the user added by hand is protected. Every other owned row is
  // endpoint-owned: an id this reply no longer names leaves the list, so the
  // stored catalog keeps matching what the provider actually serves. A row the
  // reply still names is stamped discovered, so a list written before
  // provenance existed becomes self-describing after its first sync.
  const kept = explicit.flatMap((model) => {
    if (model[MODEL_SOURCE_FIELD] === 'custom') return [{ ...model }]
    if (!reported.has(textIdOf(model))) return []
    return [{ ...model, [MODEL_SOURCE_FIELD]: 'discovered' satisfies ModelSource }]
  })
  const additions = discovered
    .filter(model => !served.has(model.id))
    .map(adoptDiscoveredModel)
  return {
    models: [...kept, ...additions],
    added: additions.length,
    removed: explicit.length - kept.length,
  }
}

/** One row's model id as stored, or the empty string when it carries none. */
function textIdOf(model: DeepSeekModelDraft): string {
  const id = model['id']
  return typeof id === 'string' ? id : ''
}

/**
 * Store one route's served model array. A fetch only ever writes the rows it
 * adopted alongside the ones already served, so the array is always non-empty:
 * an empty `models` would tell the adapter to serve nothing at all.
 * @param profilePath - path of the route's profile inside its namespace.
 * @param models - the rows the route serves.
 * @returns the ordered ops.
 */
export function modelsArrayOps(
  profilePath: readonly string[],
  models: readonly DeepSeekModelDraft[],
): SettingsPathOpView[] {
  return [{
    op: 'set',
    path: [...profilePath, 'models'],
    value: models.map(model => ({ ...model })) as JsonValue,
  }]
}

/**
 * The order a route may keep after its served list changed: an id the route no
 * longer serves cannot stay in `modelOrder`, because both adapters refuse an
 * order naming a model they do not serve.
 * @param order - the stored model order.
 * @param models - the rows that remain served.
 * @returns the ids still served, in their stored order.
 */
export function prunedModelOrder(
  order: readonly string[],
  models: readonly DeepSeekModelDraft[],
): string[] {
  const served = new Set(models.map(textIdOf))
  return order.filter(id => served.has(id))
}

/**
 * Keep a route's stored model order consistent with the rows a fetch left.
 * @param profilePath - path of the route's profile inside its namespace.
 * @param order - the stored model order, empty when the route keeps configuration order.
 * @param models - the rows the sync keeps.
 * @returns the ops that rewrite or clear an order naming a removed row; empty when every id survives.
 */
export function modelOrderSyncOps(
  profilePath: readonly string[],
  order: readonly string[],
  models: readonly DeepSeekModelDraft[],
): SettingsPathOpView[] {
  const kept = prunedModelOrder(order, models)
  if (kept.length === order.length) return []
  return kept.length === 0
    ? [{ op: 'unset', path: [...profilePath, 'modelOrder'] }]
    : [{ op: 'set', path: [...profilePath, 'modelOrder'], value: [...kept] }]
}

/**
 * Enable or disable one provider route.
 * @param profilePath - path of the route's profile inside its namespace; empty when the whole section is the profile.
 * @param enabled - the state the user asked for.
 * @returns the ordered ops, empty when nothing changes.
 */
export function providerEnabledOps(profilePath: readonly string[], enabled: boolean): SettingsPathOpView[] {
  return enabled
    ? [{ op: 'unset', path: [...profilePath, 'enabled'] }]
    : [{ op: 'set', path: [...profilePath, 'enabled'], value: false }]
}

/**
 * Enable or disable one model entry of a route.
 * @param modelsPath - path of the route's `models` array, or undefined when the route serves the
 * installed catalog and has no list to address.
 * @param index - position of the entry within that list.
 * @param enabled - the state the user asked for.
 * @returns the ordered ops; empty when a catalog route has no list entry to name.
 */
export function modelEnabledOps(
  modelsPath: readonly string[] | undefined,
  index: number,
  enabled: boolean,
): SettingsPathOpView[] {
  // A catalog route lists nothing, so a withdrawn catalog model is recorded as
  // an override instead — that is the mechanism the adapter already merges.
  if (modelsPath === undefined) return []
  return enabled
    ? [{ op: 'unset', path: [...modelsPath, String(index), 'enabled'] }]
    : [{ op: 'set', path: [...modelsPath, String(index), 'enabled'], value: false }]
}

/**
 * Record one catalog route's withdrawn models as sparse overrides.
 * @param overridesPath - path of the route's `modelOverrides` dict.
 * @param disabled - model ids the user withdrew, in catalog order.
 * @returns the ops that make the stored state match, unsetting ids re-enabled.
 */
export function disabledModelOverrideOps(overridesPath: readonly string[], disabled: readonly string[]): SettingsPathOpView[] {
  return disabled.map(id => ({ op: 'set' as const, path: [...overridesPath, id, 'enabled'], value: false }))
}

/**
 * Store the order a route serves its models in.
 * @param profilePath - path of the route's profile.
 * @param order - model ids in the order the user arranged them; empty clears the choice.
 * @returns the ordered ops.
 */
export function modelOrderOps(profilePath: readonly string[], order: readonly string[]): SettingsPathOpView[] {
  return order.length === 0
    ? [{ op: 'unset', path: [...profilePath, 'modelOrder'] }]
    : [{ op: 'set', path: [...profilePath, 'modelOrder'], value: [...order] }]
}

/**
 * Store the order the page lists configured routes in.
 * @param order - route keys in the order the user arranged them; empty clears the choice.
 * @returns the ordered ops against the namespace's `providerOrder` section field.
 */
export function providerOrderOps(order: readonly string[]): SettingsPathOpView[] {
  return order.length === 0
    ? [{ op: 'unset', path: ['providerOrder'] }]
    : [{ op: 'set', path: ['providerOrder'], value: [...order] }]
}

/**
 * Move one item of a list to a new position, leaving the rest in order.
 * @param items - the list as currently shown.
 * @param from - index of the item being moved.
 * @param to - index it lands at.
 * @returns the reordered list; `items` unchanged when either index is out of range.
 */
export function moveItem<T>(items: readonly T[], from: number, to: number): T[] {
  if (from < 0 || from >= items.length || to < 0 || to >= items.length) return [...items]
  const moved = items.slice(from, from + 1)
  const kept = items.filter((_, index) => index !== from)
  return [...kept.slice(0, to), ...moved, ...kept.slice(to)]
}
