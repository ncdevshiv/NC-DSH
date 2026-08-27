/**
 * Service Definition for the dismissible-notification seam (`ctx.notifications`).
 * Any plugin publishes user-visible notifications under a stable id; the seam
 * owns read/dismiss/delete state, persists it as one JSON document at
 * `<harness home>/notifications/v1/state.json`, and announces every committed
 * mutation through `notifications/updated` and `notifications/removed`.
 *
 * Publishing an existing id replaces its content, resets `dismissed`/`read`,
 * and preserves `createdAt`. State loads lazily on first access; a corrupt
 * store starts empty with one warning. Every write is a synchronous atomic
 * file replacement, so mutations are durable when the call returns.
 * @module @deepseek-ai/dsh-notifications
 */

import { Context, Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import { join, resolve } from 'node:path'
import { resolveDshHome } from '@deepseek-ai/dsh-home-paths'
import { parseStore, readStoreText, renderStore, writeFileAtomicSync } from './persist.ts'
import type { NotificationPublishInput, NotificationView } from './types.ts'

export type { NotificationPublishInput, NotificationView }

/** Mutable internal row; handed-out views are frozen copies. */
type NotificationRecord = NotificationView

/** Plugin config: harness-home override for the state file location. */
export interface Config {
  /** Harness home used instead of `$DSH_HOME` / `~/.dsh`. */
  dshHome?: string
}

declare module '@deepseek-ai/cordis' {
  interface Context {
    notifications: NotificationsService
  }

  interface Events {
    /**
     * A notification was published or replaced, or its read/dismiss state
     * changed: the id is present in {@linkcode NotificationsService.list}.
     * @param id - the notification id whose entry changed.
     * @mode emit
     */
    'notifications/updated'(id: string): void
    /**
     * A notification was deleted: the id is absent from
     * {@linkcode NotificationsService.list}.
     * @param id - the deleted notification id.
     * @mode emit
     */
    'notifications/removed'(id: string): void
  }
}

/** Directory segments of the versioned store below the resolved harness home. */
const STORE_SEGMENTS = ['notifications', 'v1', 'state.json'] as const

/** File-backed dismissible-notification service. */
export class NotificationsService extends Service {
  static Config: z<Config> = z.object({
    dshHome: z.string(),
  })

  /** Absolute path of the versioned store document. */
  readonly storeFile: string

  private records: NotificationRecord[] | undefined
  private warnedCorrupt = false

  /**
   * @param ctx - Cordis context owning this service.
   * @param config - harness-home override; omitted follows `$DSH_HOME`, then `~/.dsh`.
   */
  constructor(ctx: Context, config: Config = {}) {
    super(ctx, 'notifications')
    this.storeFile = resolve(join(resolveDshHome(config.dshHome), ...STORE_SEGMENTS))
  }

  /** Load the store on first access; corruption starts empty with one warning. */
  private ensureLoaded(): NotificationRecord[] {
    if (this.records !== undefined) return this.records
    const text = readStoreText(this.storeFile)
    if (text === undefined) {
      this.records = []
      return this.records
    }
    try {
      this.records = parseStore(text)
    } catch (error) {
      // The document cannot be trusted in part or whole, so the seam serves an
      // empty store rather than guessing; the next mutation rewrites it clean.
      this.warnCorrupt(error)
      this.records = []
    }
    return this.records
  }

  /** Warn once per process about the same corrupt store. */
  private warnCorrupt(error: unknown): void {
    /* v8 ignore next -- one lazy load parses one document, so a single instance can never warn twice */
    if (this.warnedCorrupt) return
    this.warnedCorrupt = true
    this.ctx.logger.warn('notifications: ignoring corrupt store at %s', this.storeFile)
    this.ctx.logger.warn(error)
  }

  /** Commit the given rows to disk atomically. */
  private persist(rows: NotificationRecord[]): void {
    writeFileAtomicSync(this.storeFile, renderStore(rows))
  }

  /**
   * Publish one notification, or replace the content of the existing entry
   * with the same id. Replacement keeps `createdAt` and resets `dismissed`
   * and `read` to `false`; the input is cloned, so later caller mutations of
   * `data` never reach stored or handed-out views.
   * @param input - identity and content of the notification.
   */
  publish(input: NotificationPublishInput): void {
    const rows = this.ensureLoaded()
    const existingIndex = rows.findIndex(row => row.id === input.id)
    const previous = existingIndex === -1 ? undefined : rows[existingIndex]
    const record: NotificationRecord = {
      id: input.id,
      kind: input.kind,
      title: input.title,
      ...(input.body === undefined ? {} : { body: input.body }),
      ...(input.data === undefined ? {} : { data: structuredClone(input.data) }),
      createdAt: previous?.createdAt ?? new Date().toISOString(),
      read: false,
      dismissed: false,
    }
    if (previous === undefined) rows.push(record)
    else rows[existingIndex] = record
    this.persist(rows)
    this.emitMutation('notifications/updated', input.id)
  }

  /**
   * Snapshot every live notification, newest first (reverse insertion order;
   * replacement retains its original position). Views are frozen clones, so
   * caller mutation never reaches the store.
   * @returns frozen views ordered newest first.
   */
  list(): readonly NotificationView[] {
    const rows = this.ensureLoaded()
    return Object.freeze([...rows].reverse().map(freezeView))
  }

  /**
   * Mark one notification read or unread. An unknown id fails loud; setting
   * the current value again changes nothing and emits nothing.
   * @param id - the notification to update.
   * @param read - next read state; defaults to `true`.
   */
  setRead(id: string, read: boolean = true): void {
    const rows = this.ensureLoaded()
    const record = rows.find(row => row.id === id)
    if (record === undefined) throw new Error(`notifications: unknown id "${id}"`)
    if (record.read === read) return
    record.read = read
    this.persist(rows)
    this.emitMutation('notifications/updated', id)
  }

  /**
   * Dismiss one notification (keeps it listed with `dismissed: true`). An
   * unknown id fails loud; dismissing twice changes nothing.
   * @param id - the notification to dismiss.
   */
  dismiss(id: string): void {
    const rows = this.ensureLoaded()
    const record = rows.find(row => row.id === id)
    if (record === undefined) throw new Error(`notifications: unknown id "${id}"`)
    if (record.dismissed) return
    record.dismissed = true
    this.persist(rows)
    this.emitMutation('notifications/updated', id)
  }

  /**
   * Delete one notification. Deleting an absent id is already satisfied.
   * @param id - the notification to delete.
   */
  delete(id: string): void {
    const rows = this.ensureLoaded()
    const remaining = rows.filter(row => row.id !== id)
    if (remaining.length === rows.length) return
    this.records = remaining
    this.persist(remaining)
    this.emitMutation('notifications/removed', id)
  }

  /**
   * Fan one committed mutation out contained: each listener runs
   * independently, sync throws and async rejections are logged, and an
   * INVARIANT-coded failure still surfaces after every listener ran (the
   * registry-notification contract shared by settings and llm).
   */
  private emitMutation(name: 'notifications/updated' | 'notifications/removed', id: string): void {
    let invariantFailure: unknown
    for (const listener of this.ctx.events.dispatch('emit', [name, id]) as Array<(id: string) => unknown>) {
      try {
        const returned = listener(id)
        if (returned != null && typeof (returned as PromiseLike<unknown>).then === 'function') {
          void Promise.resolve(returned as PromiseLike<unknown>).then(undefined, (error: unknown) => {
            this.warnListenerFailure(name, error)
          })
        }
      } catch (error) {
        if ((error as { code?: unknown } | null)?.code === 'INVARIANT') {
          invariantFailure ??= error
          continue
        }
        this.warnListenerFailure(name, error)
      }
    }
    if (invariantFailure !== undefined) throw invariantFailure as Error
  }

  /** Contained-listener diagnostic shared by the sync and async failure paths. */
  private warnListenerFailure(name: string, error: unknown): void {
    this.ctx.logger.warn('notifications: a %s listener failed', name)
    this.ctx.logger.warn(error)
  }
}

/** Freeze one row shallowly plus its data payload before handing it out. */
function freezeView(record: NotificationRecord): NotificationView {
  const view: NotificationRecord = { ...record }
  if (view.data !== undefined) view.data = Object.freeze(view.data)
  return Object.freeze(view)
}

export default NotificationsService
