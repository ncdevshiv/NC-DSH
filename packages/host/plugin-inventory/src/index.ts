/** Read-only projection of the current Cordis Loader plugin entries. */

import type { Context, FiberState } from '@deepseek-ai/cordis'
import type { Entry } from '@deepseek-ai/cordis-plugin-loader'
import { TypertRemoteService, Remote } from '@deepseek-ai/dsh-typert-protocol'
// Typert-generated ./typert and ./remote artifacts import Zod at runtime.
import type {} from 'zod'
import type {
  PluginEntryId,
  PluginFiberPhase,
  PluginInventoryEntry,
  PluginInventorySnapshot,
} from './types.ts'

export type * from './types.ts'

/** Brand an existing Loader-tree entry id at the owning boundary. */
function pluginEntryId(value: string): PluginEntryId {
  return value as PluginEntryId
}

/** Render a thrown value as the single-line message the Remote payload carries. */
function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/** Runtime mirror: FiberState is a cross-package const enum. */
const FIBER_STATE = {
  PENDING: 0 as FiberState.PENDING,
  LOADING: 1 as FiberState.LOADING,
  ACTIVE: 2 as FiberState.ACTIVE,
  FAILED: 3 as FiberState.FAILED,
  DISPOSED: 4 as FiberState.DISPOSED,
  UNLOADING: 5 as FiberState.UNLOADING,
} as const

/** Complete public projection of Cordis Fiber states. */
const FIBER_PHASE = {
  [FIBER_STATE.PENDING]: 'pending',
  [FIBER_STATE.LOADING]: 'loading',
  [FIBER_STATE.ACTIVE]: 'active',
  [FIBER_STATE.FAILED]: 'failed',
  [FIBER_STATE.DISPOSED]: null,
  [FIBER_STATE.UNLOADING]: 'unloading',
} as const satisfies Record<FiberState, PluginFiberPhase>

/** Remote-only service exposing the Loader's current non-group entry state. */
export class PluginInventoryGateway extends TypertRemoteService {
  static inject = ['loader']

  constructor(ctx: Context) {
    super(ctx, 'pluginInventory')
  }

  /**
   * Read the Loader directly on every call. Cordis's internal plugin/status
   * events already maintain Entry.fiber and Fiber.state, so a second cache
   * would only add another lifecycle truth to keep synchronized.
   * @returns Current non-group Loader entries in Loader order.
   */
  @Remote('list')
  list(): PluginInventorySnapshot {
    const entries: PluginInventoryEntry[] = []
    for (const entry of this.ctx.loader.entries()) {
      if (entry.options.group) continue
      entries.push({
        entryId: pluginEntryId(entry.id),
        moduleName: entry.options.name,
        enabled: !entry.disabled,
        fiberPhase: entry.fiber === undefined ? null : FIBER_PHASE[entry.fiber.state],
      })
    }
    return { entries }
  }

  /**
   * Enable or disable one Loader entry. The change is live for this process;
   * the underlying patch file is not rewritten here so the caller can decide
   * whether the toggle should survive a restart.
   *
   * Enabling first applies the entry directly. When that apply collides with
   * an active same-module entry — two mounted instances cannot publish the
   * same service in one realm, so Cordis rejects the duplicate registration —
   * this gateway replaces instead of failing: it disables the active
   * duplicates, retries the enable once, and restores every displaced entry
   * when the replacement itself fails. Displacement triggers only after a
   * real failed enable, so entries providing distinct isolated instances are
   * never disturbed.
   * @param entryId - the Loader-tree id of the entry to mutate.
   * @param enabled - true to enable, false to disable.
   * @returns whether the Loader accepted the change.
   */
  @Remote('setEnabled')
  async setEnabled(entryId: PluginEntryId, enabled: boolean): Promise<{ ok: boolean; message?: string }> {
    let entry
    try {
      entry = this.ctx.loader.resolve(entryId)
    } catch (error) {
      return { ok: false, message: errorMessage(error) }
    }
    if (entry.options.group) {
      return { ok: false, message: 'Group entries cannot be toggled' }
    }
    const currentlyEnabled = !entry.disabled
    if (currentlyEnabled === enabled) return { ok: true }
    try {
      // `null` removes the disabled key so the patch file does not accumulate
      // `disabled: false` entries; `true` disables.
      await this.ctx.loader.update(entryId, { disabled: enabled ? null : true })
      return { ok: true }
    } catch (error) {
      if (!enabled) return { ok: false, message: errorMessage(error) }
      return await this.enableReplacingActiveDuplicates(entry, error)
    }
  }

  /**
   * Enable `entry` by first disabling its active same-module duplicates and
   * re-applying the enable once. A failed displacement or a failed retry
   * restores the previously active set before the failure is reported.
   * @param entry - the resolved target entry.
   * @param failure - the error from the direct enable attempt.
   * @returns whether the replacement succeeded.
   */
  private async enableReplacingActiveDuplicates(entry: Entry, failure: unknown): Promise<{ ok: boolean; message?: string }> {
    const duplicates = [...this.ctx.loader.entries()].filter(candidate =>
      candidate !== entry
      && !candidate.options.group
      && !candidate.disabled
      && candidate.fiber !== undefined
      && candidate.fiber.state === FIBER_STATE.ACTIVE
      && candidate.options.name === entry.options.name,
    )
    if (duplicates.length === 0) return { ok: false, message: errorMessage(failure) }
    const displaced: Entry[] = []
    for (const duplicate of duplicates) {
      try {
        await this.ctx.loader.update(duplicate.id, { disabled: true })
        displaced.push(duplicate)
      } catch (error) {
        const restored = await this.restoreEntries(displaced)
        return {
          ok: false,
          message: `cannot replace running duplicate ${duplicate.id}: ${errorMessage(error)}${restored}`,
        }
      }
    }
    try {
      await this.ctx.loader.update(entry.id, { disabled: null })
      return { ok: true }
    } catch (error) {
      const restored = await this.restoreEntries(displaced)
      return { ok: false, message: `${errorMessage(error)}${restored}` }
    }
  }

  /**
   * Re-enable displaced entries best effort; their modules were running
   * moments ago, so restoration normally succeeds.
   * @param entries - the entries displaced for the failed replacement.
   * @returns `''`, or a note naming each entry that could not be restored.
   */
  private async restoreEntries(entries: readonly Entry[]): Promise<string> {
    const failures: string[] = []
    for (const entry of entries) {
      try {
        await this.ctx.loader.update(entry.id, { disabled: null })
      } catch (error) {
        failures.push(`${entry.id}: ${errorMessage(error)}`)
      }
    }
    return failures.length === 0
      ? ''
      : ` (previous providers not fully restored: ${failures.join('; ')})`
  }
}

export default PluginInventoryGateway
