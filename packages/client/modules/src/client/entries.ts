/** Page-owned Loader entries; transport-independent reconciliation, retries and code replacement. */
import type { FiberState } from '@deepseek-ai/cordis'
import type { Entry, Loader } from '@deepseek-ai/cordis-plugin-loader'
import type { ObservableSnapshot } from '@deepseek-ai/dsh-client-store'
import { parseBootManifest, readRelease } from './manifest.ts'
import type {
  BootManifest, ClientBundleRegistration, ClientModuleLoader, ClientModuleRecord,
} from './manifest.ts'
import { ReleaseChangeGate, type ClientReleaseChange } from './release-change.ts'
import { notifySubscribers } from './notify.ts'
import { detachOwnedStyles, removeOwnedStyles, restoreOwnedStyles, tearDownEntryFiber } from './entry-lifecycle.ts'

/** Page-local failures do not change the Host's bundle enablement. */
export interface ClientEntryState {
  /** True while a snapshot, retry or code replacement is being applied. */
  readonly syncing: boolean
  /** Package ids and errors from the latest reconciliation. */
  readonly failures: readonly { readonly id: string; readonly message: string }[]
}

/** Module-table state of one running package, restorable after a failed live replacement. */
export interface ClientPackageSnapshot {
  /** Registered factories, including package-local chunks. */
  readonly factories: readonly (readonly [string, {
    readonly factory: ClientBundleRegistration['factory']
    readonly rev: string | undefined
  }])[]
  /** Materialized module records, including package-local chunks. */
  readonly records: readonly (readonly [string, ClientModuleRecord])[]
  /** Pending single-resource reload target, when the captured code had one. */
  readonly reloadTarget: { readonly url: string; readonly rev: string } | undefined
}

/** Module-table capabilities used within serialized entry operations. */
interface ModuleIndex {
  /** @returns the ids of rows the new graph carries at a revision other than the running code. */
  update(manifest: BootManifest, managed: Iterable<string>): Set<string>
  invalidateForReplacement(id: string, rev: string): void
  /** Rows that import the given rows, directly or through a chain, excluding bootstrap rows. */
  consumersOf(ids: Iterable<string>): Set<string>
  /** Discard one package's materialized records, keeping its factory, so a re-import reads the current table. */
  invalidateRecord(id: string): void
  /** Capture the module state of the code running now. */
  snapshot(id: string): ClientPackageSnapshot
  /** Restore one captured package state after a failed replacement. */
  restore(id: string, snapshot: ClientPackageSnapshot): void
  prune(roots: Iterable<string>): void
}

/** Numeric values mirror Cordis's const enum, which bundle loaders cannot import as a runtime object. */
const ACTIVE = 2 as FiberState.ACTIVE
const FAILED = 3 as FiberState.FAILED

/** Revisions and requests identify desired code; URLs only select its immutable delivery resource. */
function entryTargets(manifest: BootManifest): string {
  return JSON.stringify(manifest.modules.map(row => [row.id, row.rev, row.inject, row.external]))
}

/** Manages only entries created from the Host manifest; other Loader contributors retain ownership. */
export class ClientEntries {
  /** Stable observable consumed by page diagnostics through the renderer's injected hook. */
  readonly state: ObservableSnapshot<ClientEntryState> = {
    getSnapshot: () => this.snapshot,
    subscribe: (listener) => {
      this.listeners.add(listener)
      return () => { this.listeners.delete(listener) }
    },
  }
  // The modules bootstrap factory cannot request platform libraries before the shell supplies its seed.
  private snapshot: ClientEntryState = { syncing: false, failures: [] }
  private readonly listeners = new Set<() => void>()
  /** Admits only Host graphs of the page's boot release. */
  private readonly release: ReleaseChangeGate
  private readonly managed = new Map<string, Entry>()
  private readonly revisions = new Map<string, string>()
  /** Module-table state of the last code each package activated successfully. */
  private readonly lastGood = new Map<string, ClientPackageSnapshot>()
  private loader: Loader | undefined
  private queue: Promise<void> = Promise.resolve()
  private desired: BootManifest
  private generation = 0
  private stopped = false

  /**
   * Construct the page controller before Cordis boot.
   * @param modules - Module arrival and materialization owner.
   * @param index - Private descriptor replacement and unused-module cleanup.
   */
  constructor(private readonly modules: ClientModuleLoader, private readonly index: ModuleIndex) {
    this.desired = modules.manifest
    this.release = new ReleaseChangeGate(modules.manifest.release)
  }

  /** Host release change to announce; undefined while the Host serves the page's boot release or a dismissed one. */
  get releaseChange(): ObservableSnapshot<ClientReleaseChange | undefined> {
    return this.release.change
  }

  /** Stop announcing the connected Host's release; updates stay paused until the Host returns to the boot release or the page reloads. */
  dismissRelease(): void {
    this.release.dismiss()
  }

  /**
   * Create the initial roster and retain its entry identities for subsequent reconciliation.
   * @param loader - Page Loader, already configured with the module system.
   * @param manifest - Initial roster audited by the boot caller.
   * @returns after initial entries and their activation settle; boot owns its activation audit.
   */
  start(loader: Loader, manifest: BootManifest): Promise<void> {
    if (this.loader !== undefined) throw new Error('client-modules: entries already started')
    this.loader = loader
    this.desired = manifest
    loader.ctx.effect(() => () => {
      this.stopped = true
      this.generation++
      return this.queue
    }, 'client-modules: entry reconciliation')
    return this.enqueue(async () => {
      await Promise.all(this.desired.plugins.map(async ({ id }) => {
        await this.create(loader, id)
      }))
      await loader.await()
      for (const row of this.modules.manifest.modules) this.revisions.set(row.id, row.rev)
      for (const id of this.managed.keys()) this.remember(id)
    })
  }

  /**
   * Validate and apply the latest full Host graph. Changed targets cancel obsolete mounts; identical targets share pending loads.
   * The release is compared before the rest of the graph is validated: a graph of another release, including one whose
   * format this page cannot read, is not applied and {@link releaseChange} announces it.
   * @param graph - JSON-decoded graph received from the Host.
   * @returns after the queued reconciliation, or at once for another release; per-package failures remain available in {@link state}.
   */
  sync(graph: unknown): Promise<void> {
    if (!this.release.observe(readRelease(graph))) return Promise.resolve()
    const manifest = parseBootManifest(graph)
    if (entryTargets(manifest) !== entryTargets(this.desired)) this.generation++
    this.desired = manifest
    const generation = this.generation
    return this.enqueue(() => this.reconcile(generation))
  }

  /**
   * Retry failed entries against the latest graph, including an unchanged revision.
   * @returns after retry settlement, with remaining errors in {@link state}.
   */
  retry(): Promise<void> {
    const generation = ++this.generation
    return this.enqueue(() => this.reconcile(generation))
  }

  /**
   * Replace one entry's code in the same queue as graph updates; duplicate revisions are ignored.
   * Entries missing after a failed import are reconciled; bootstrap replacement fails before teardown.
   * Ignored while the connected Host serves another release than the page's boot release.
   * @param id - Package id from a rebuilt frame.
   * @param rev - Opaque revision selecting the rebuilt artifact.
   * @returns after queued work; replacement errors reject, while per-package reconciliation errors remain in {@link state}.
   */
  reload(id: string, rev: string): Promise<void> {
    if (!this.release.matches) return Promise.resolve()
    this.desired = {
      ...this.desired,
      modules: this.desired.modules.map(row => row.id === id ? { ...row, rev } : row),
    }
    return this.enqueue(async () => {
      const desired = this.desired.modules.find(row => row.id === id)
      if (this.stopped || desired === undefined) return
      const entry = this.managed.get(id)
      if (entry === undefined) {
        this.modules.invalidate(id, desired.rev)
        removeOwnedStyles(id)
        await this.reconcile(this.generation)
        return
      }
      if (this.revisions.get(id) === rev) return
      this.publish({ syncing: true, failures: this.snapshot.failures.filter(failure => failure.id !== id) })
      await this.replace(entry, id, rev, this.generation)
      const failures: { id: string; message: string }[] = []
      await this.refreshConsumers(new Set([id]), failures, this.generation)
      this.publish({
        syncing: false,
        failures: [
          ...this.snapshot.failures.filter(failure => !failures.some(retry => retry.id === failure.id)),
          ...failures,
        ],
      })
    }, id)
  }

  private publish(snapshot: ClientEntryState): void {
    this.snapshot = snapshot
    notifySubscribers(this.listeners, 'synchronization')
  }

  private enqueue(task: () => Promise<void>, subject = 'graph'): Promise<void> {
    const run = this.queue.then(task)
    // Each operation reports its own failure; later operations must still run.
    this.queue = run.then(() => undefined, (error: unknown) => {
      this.publish({ syncing: false, failures: [
        ...this.snapshot.failures.filter(failure => failure.id !== subject),
        { id: subject, message: String(error) },
      ] })
    })
    return run
  }

  private current(generation: number): boolean {
    return !this.stopped && generation === this.generation
  }

  /** Keep ownership even when Loader rejects a module's plugin exports after inserting its entry. */
  private async create(loader: Loader, id: string): Promise<void> {
    const options = { name: id }
    const entryId = loader.ensureId(options)
    try {
      await loader.create(options)
    } finally {
      this.managed.set(id, loader.resolve(entryId))
    }
  }

  /**
   * Put one entry onto new code. `redeliver` fetches the artifact revision named by `rev`,
   * which is what a rebuilt or superseded row needs. `rematerialize` keeps the delivered
   * artifact and re-runs its registered factory, which is what an unchanged consumer of a
   * rebuilt dependency needs: its own bytes are current, only the exports it captured are not.
   * @param entry - managed entry to move onto the new code.
   * @param id - package id.
   * @param rev - revision the entry runs after a successful swap.
   * @param generation - reconciliation or reload this pass still owns.
   * @param mode - whether the artifact is fetched again or re-run from the table.
   */
  private async replace(
    entry: Entry,
    id: string,
    rev: string,
    generation: number,
    mode: 'redeliver' | 'rematerialize' = 'redeliver',
  ): Promise<void> {
    const previous = this.lastGood.get(id)
    if (mode === 'redeliver') {
      this.index.invalidateForReplacement(id, rev)
      await this.modules.prefetch(id)
    } else {
      this.index.invalidateRecord(id)
    }
    if (!this.current(generation)) return
    await tearDownEntryFiber(entry)
    const detachedStyles = detachOwnedStyles(id)
    if (!this.current(generation)) {
      restoreOwnedStyles(detachedStyles)
      return
    }
    try {
      await this.modules.import(id, '', {})
      if (!this.current(generation)) return
      await entry.refresh()
      await entry.fiber?.await()
      if (entry.fiber === undefined) throw new Error(`client-modules: ${id} import failed (see console)`)
      this.revisions.set(id, rev)
      this.remember(id)
    } catch (error: unknown) {
      if (!this.current(generation) || previous === undefined) throw error
      await this.rollbackReplacement(entry, id, previous, detachedStyles, error)
      throw error
    }
  }

  /** Retain the module-table state of the code a package is currently running. */
  private remember(id: string): void {
    const entry = this.managed.get(id)
    if (entry?.fiber?.state !== ACTIVE) return
    this.lastGood.set(id, this.index.snapshot(id))
  }

  /**
   * Return a package to its last activated code after a live replacement fails,
   * so a bad edit leaves the page running the previous bundle. The replacement's
   * own fiber and styles are discarded first; styles detached from the
   * still-active code are re-attached after the module table is restored.
   */
  private async rollbackReplacement(
    entry: Entry,
    id: string,
    snapshot: ClientPackageSnapshot,
    styles: readonly Element[],
    failure: unknown,
  ): Promise<void> {
    await tearDownEntryFiber(entry)
    removeOwnedStyles(id)
    this.index.restore(id, snapshot)
    restoreOwnedStyles(styles)
    try {
      await entry.refresh()
      await entry.fiber?.await()
      if (entry.fiber === undefined) throw new Error(`client-modules: ${id} rollback could not restore the previous code`)
    } catch (rollbackError: unknown) {
      // The replacement failure is what the page reports; the rollback failure
      // stays attached as its cause instead of replacing the original message.
      throw new Error(failure instanceof Error ? failure.message : String(failure), {
        cause: new AggregateError(
          [failure, rollbackError],
          `client-modules: ${id} replacement failed and rollback could not restore the previous code`,
        ),
      })
    }
  }

  /**
   * Replace the managed entries that import freshly loaded code, so an edit to a
   * shared bundle reaches the entries that captured its exports when they
   * materialized. Rows are refreshed in graph order, which places every provider
   * before its consumers, and only the reached entries refresh — an unrelated
   * plugin keeps running untouched. A consumer whose refresh fails reports that
   * entry and keeps its previous code, because {@link replace} rolls the attempt
   * back; the seeds of a failed replacement stay out of `refreshed`, so its
   * consumers keep the pair they already run.
   * @param refreshed - ids whose module table now holds new code.
   * @param failures - reconciliation failures, extended with each failed consumer.
   * @param generation - reconciliation this pass still owns.
   */
  private async refreshConsumers(
    refreshed: ReadonlySet<string>,
    failures: { id: string; message: string }[],
    generation: number,
  ): Promise<void> {
    if (refreshed.size === 0) return
    const stale = this.index.consumersOf(refreshed)
    for (const row of this.desired.modules) {
      if (!this.current(generation)) return
      if (!stale.has(row.id) || refreshed.has(row.id)) continue
      if (failures.some(failure => failure.id === row.id)) continue
      const entry = this.managed.get(row.id)
      if (entry === undefined) continue
      try {
        await this.replace(entry, row.id, row.rev, generation, 'rematerialize')
      } catch (error: unknown) {
        failures.push({ id: row.id, message: String(error) })
      }
    }
  }

  private async reconcile(generation: number): Promise<void> {
    if (!this.current(generation)) return
    const loader = this.loader
    if (loader === undefined) throw new Error('client-modules: entries have not started')
    const manifest = this.desired
    this.publish({ syncing: true, failures: [] })
    const failures: { id: string; message: string }[] = []
    const refreshed = this.index.update(manifest, this.managed.keys())
    const wanted = new Set(manifest.plugins.map(row => row.id))
    for (const [id, entry] of this.managed) {
      if (wanted.has(id)) continue
      const fiber = entry.fiber
      loader.remove(entry.id)
      this.managed.delete(id)
      this.revisions.delete(id)
      this.lastGood.delete(id)
      // Removed fibers no longer appear in Loader.getTasks().
      while (fiber?.inertia !== undefined) await fiber.inertia
    }
    for (const row of manifest.modules) {
      if (!this.current(generation)) break
      try {
        const entry = this.managed.get(row.id)
        if (entry === undefined) {
          await this.modules.prefetch(row.id)
          if (!this.current(generation)) break
          await this.modules.import(row.id, '', {})
          if (!this.current(generation)) break
          await this.create(loader, row.id)
          this.revisions.set(row.id, row.rev)
        } else if (this.revisions.get(row.id) !== row.rev || entry.fiber === undefined) {
          await this.replace(entry, row.id, row.rev, generation)
          refreshed.add(row.id)
        } else if (entry.fiber.state === FAILED) {
          entry.fiber.update(entry.options.config)
        }
      } catch (error) {
        failures.push({ id: row.id, message: String(error) })
      }
    }
    await this.refreshConsumers(refreshed, failures, generation)
    await loader.await()
    for (const [id, entry] of this.managed) {
      if (failures.some(failure => failure.id === id)) continue
      if (entry.fiber?.state === ACTIVE) continue
      try {
        if (entry.fiber === undefined) throw new Error(`client-modules: ${id} import failed (see console)`)
        await entry.fiber.await()
        failures.push({ id, message: `client-modules: ${id} is waiting for activation` })
      } catch (error) {
        failures.push({ id, message: String(error) })
      }
    }
    for (const [id] of this.managed) {
      if (failures.some(failure => failure.id === id)) continue
      this.remember(id)
    }
    this.index.prune([...loader.entries()].map(entry => entry.options.name))
    if (this.current(generation)) this.publish({ syncing: false, failures })
  }
}
