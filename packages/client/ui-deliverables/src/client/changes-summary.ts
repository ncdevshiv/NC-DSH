/** Fetch-once cache of the change summaries the Host serves for announced `workspace/changes` events. */
import type { SessionId } from '@deepseek-ai/dsh-session/types'
import { changesSummaryUrl, isChangesSummary, type ChangesSummary } from '../changes.ts'
import { HostReadStore } from './host-read-store.ts'

/**
 * A summary, `'missing'` once the Host no longer serves it, `'error'` for a
 * failed read a later request retries, or `'loading'` while the request runs.
 */
export type ChangesSummaryState = ChangesSummary | 'missing' | 'error' | 'loading'

/** One browser plugin's summary reads; a summary or a missing answer is kept until the connection is replaced. */
export class ChangesSummaryStore extends HostReadStore<ChangesSummaryState> {
  constructor() {
    super({
      loading: 'loading',
      failed: 'error',
      retryable: state => state === 'error',
      decode: async (response) => {
        if (response.status === 404) return 'missing'
        if (!response.ok) return 'error'
        const value: unknown = await response.json()
        return isChangesSummary(value) ? value : 'error'
      },
    })
  }

  /**
   * Read one summary once; a later read of the same coordinates returns the cached state.
   * @param sessionId - viewed Session.
   * @param seq - the announcing event's sequence.
   * @returns after the state is published.
   */
  load(sessionId: SessionId, seq: number): Promise<void> {
    return this.loadUrl(changesSummaryUrl(sessionId, seq))
  }
}
