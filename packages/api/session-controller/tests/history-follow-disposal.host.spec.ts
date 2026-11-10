/**
 * Late-resuming follow containment: a Session point read that stays suspended
 * while the fiber owning the history controller is disposed.
 */

import { describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import SessionStore from '@deepseek-ai/dsh-session'
import type { Session, SessionEvent, SessionId } from '@deepseek-ai/dsh-session'
import SessionQueryEngine from '@deepseek-ai/dsh-session-query'
import type { SessionObservation, SessionObservationOptions } from '@deepseek-ai/dsh-session-query'
import { SessionHistoryController } from '@deepseek-ai/dsh-api-session-controller/src/history.ts'
import type { SessionFollowFrame } from '@deepseek-ai/dsh-api-session-controller/types'

const sid = (id: string): SessionId => id as SessionId

/** One prepared cold entry with the Session Observation reader's lease accounting. */
interface PreparedEntry {
  /** The unpublished Session restored for the read. */
  readonly session: Session
  /** The immutable event prefix the read observed. */
  readonly events: readonly SessionEvent[]
  /** Live observation leases over this entry. */
  refs: number
}

/**
 * Session query double whose point read prepares one cold entry and then holds
 * the caller behind a controlled promise. The double ignores the caller's
 * cancellation, so the read completes after the owning fiber was disposed:
 * the late-resuming shape the follow containment guard covers.
 */
class ControlledSessionQuery extends SessionQueryEngine {
  /** Resolves once the point read prepared its entry and suspended. */
  readonly entered = Promise.withResolvers<undefined>()
  /** Releases the suspended point read. */
  readonly release = Promise.withResolvers<undefined>()
  /** Prepared entries by Session identity. */
  readonly entries = new Map<SessionId, PreparedEntry>()
  /** Number of Sessions this double prepared in the Session store. */
  prepareCalls = 0
  /** The context that owns the Session store this double prepares entries in. */
  private readonly storeCtx: Context

  constructor(ctx: Context) {
    super(ctx)
    this.storeCtx = ctx
  }

  override searchSessions(): Promise<never> {
    return Promise.reject(new Error('session search is not configured in this fixture'))
  }

  override searchEvents(): Promise<never> {
    return Promise.reject(new Error('event search is not configured in this fixture'))
  }

  override async observeSession(
    sessionId: SessionId,
    _options: SessionObservationOptions = {},
  ): Promise<SessionObservation> {
    const session = this.storeCtx.sessions.prepare(sessionId, {
      seed: [],
      meta: { cwd: '/workspace', createdAt: 1000 },
    })
    this.prepareCalls += 1
    const entry: PreparedEntry = { session, events: [], refs: 0 }
    this.entries.set(sessionId, entry)
    const observation = this.lease(sessionId, entry)
    this.entered.resolve(undefined)
    await this.release.promise
    return observation
  }

  /** Live lease count of the prepared entry for one Session identity. */
  refsOf(sessionId: SessionId): number {
    return this.entries.get(sessionId)?.refs ?? 0
  }

  private lease(sessionId: SessionId, entry: PreparedEntry): SessionObservation {
    let disposed = false
    const makeLease = (): SessionObservation => ({
      source: 'prepared',
      header: entry.session.header,
      inheritedEventCount: entry.session.inheritedEventCount,
      events: entry.events,
      cursor: -1,
      retain: () => {
        if (disposed) throw new Error(`session observation "${sessionId}" is disposed`)
        entry.refs += 1
        return makeLease()
      },
      [Symbol.dispose]: () => {
        if (disposed) return
        disposed = true
        entry.refs -= 1
      },
    })
    entry.refs += 1
    return makeLease()
  }
}

describe('late-resuming follow containment', () => {
  it('takes no lease and delivers nothing when the owning fiber is disposed during the source read', async () => {
    const ctx = new Context()
    await ctx.plugin(SessionStore)
    const query = new ControlledSessionQuery(ctx)
    const promotions: SessionObservation[] = []
    let history: SessionHistoryController | undefined
    const owner = ctx.plugin({
      name: 'test:history-owner',
      apply(fiberCtx: Context): void {
        history = new SessionHistoryController(fiberCtx, (observation) => {
          promotions.push(observation)
        })
      },
    })
    await owner
    const controller = history
    if (controller === undefined) throw new Error('the history owner plugin did not start')
    const sessionId = sid('session-late-resume')
    const abort = new AbortController()
    const iterator = controller.follow({
      address: { kind: 'session', sessionId },
    }, abort.signal)[Symbol.asyncIterator]()
    const delivered: SessionFollowFrame[] = []
    const drained = (async () => {
      for await (const frame of { [Symbol.asyncIterator]: () => iterator }) {
        delivered.push(frame)
      }
    })()
    await query.entered.promise

    // Disposal begins while the point read is still suspended: the follower
    // flag flips, but the read holds its lease until it resumes, so the
    // disposal settles while the entry is still pinned.
    await owner.dispose()
    // The double prepared the cold entry before it suspended, so the count at
    // the disposal mark is the baseline: a late-resuming follow must not add a
    // preparation, a promotion lease, or a pinned entry of its own.
    const preparedAtDisposal = query.prepareCalls
    expect(preparedAtDisposal).toBe(1)
    expect(query.refsOf(sessionId)).toBe(1)

    query.release.resolve(undefined)
    try {
      await drained
      expect.soft(delivered).toEqual([])
      expect.soft(query.prepareCalls).toBe(preparedAtDisposal)
      expect.soft(query.refsOf(sessionId)).toBe(0)
      expect.soft(promotions).toHaveLength(0)
    } finally {
      query.release.resolve(undefined)
      await drained.catch(() => undefined)
      abort.abort()
      await iterator.return?.()
      await ctx.fiber.dispose()
    }
  })
})
