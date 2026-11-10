/**
 * One per-path asynchronous mutex. Task execution, lease renewal and change
 * submission run concurrently in one process, and the operations that touch a
 * workspace path must not interleave.
 *
 * Every acquisition reserves a slot before awaiting it, so two callers never
 * observe the same free path; the release removes exactly its own slot.
 * @module @deepseek-ai/dsh-ncag-orchestrator/lock
 */

interface Waiter {
  readonly resolve: () => void
  readonly reject: (error: Error) => void
  readonly signal: AbortSignal | undefined
  readonly onAbort: (() => void) | undefined
}

/**
 * Keyed mutex that serializes critical sections over one string key.
 */
export class KeyedLock {
  private readonly holders = new Set<string>()
  private readonly queues = new Map<string, Waiter[]>()

  /**
   * Run one critical section while holding the key.
   * @param key - the resource key.
   * @param body - the work to run exclusively.
   * @param signal - optional cancellation; an aborted waiter is rejected instead of granted.
   * @returns the body's result.
   */
  async run<T>(key: string, body: () => T | Promise<T>, signal?: AbortSignal): Promise<T> {
    await this.acquire(key, signal)
    try {
      return await body()
    } finally {
      this.release(key)
    }
  }

  /**
   * Whether one key is currently held.
   * @param key - the resource key.
   * @returns whether a holder exists.
   */
  held(key: string): boolean {
    return this.holders.has(key)
  }

  /** Acquire the key or wait behind the current holder. */
  private acquire(key: string, signal?: AbortSignal): Promise<void> {
    if (signal?.aborted === true) return Promise.reject(new Error(`ncag: lock acquisition for ${key} was aborted`))
    if (!this.holders.has(key)) {
      this.holders.add(key)
      return Promise.resolve()
    }
    return new Promise<void>((resolve, reject) => {
      const queue = this.queues.get(key) ?? []
      const waiter: Waiter = {
        resolve,
        reject,
        signal,
        onAbort: undefined,
      }
      if (signal !== undefined) {
        const onAbort = (): void => {
          /* v8 ignore start -- the waiter's queue entry exists while its abort listener is attached; only a defensive race can miss it. */
          const index = (this.queues.get(key) ?? []).indexOf(waiter)
          if (index >= 0) (this.queues.get(key) ?? []).splice(index, 1)
          /* v8 ignore stop */
          reject(new Error(`ncag: lock acquisition for ${key} was aborted`))
        }
        (waiter as { onAbort: (() => void) | undefined }).onAbort = onAbort
        signal.addEventListener('abort', onAbort, { once: true })
      }
      queue.push(waiter)
      this.queues.set(key, queue)
    })
  }

  /** Release the key and hand it to the next waiter, if any. */
  private release(key: string): void {
    const queue = this.queues.get(key)
    const next = queue?.shift()
    if (next === undefined) {
      this.holders.delete(key)
      this.queues.delete(key)
      return
    }
    if (next.onAbort !== undefined) next.signal?.removeEventListener('abort', next.onAbort)
    next.resolve()
  }
}
