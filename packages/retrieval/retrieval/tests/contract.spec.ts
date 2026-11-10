import { describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import RetrievalService, {
  RetrievalError,
  type RetrievalHit,
  type RetrievalSearchPage,
  type RetrievalStats,
} from '@deepseek-ai/dsh-retrieval'

class StubRetrievalService extends RetrievalService {
  readonly calls: string[] = []

  override async indexDocuments(): Promise<void> {
    this.calls.push('indexDocuments')
  }

  override async removeDocuments(): Promise<void> {
    this.calls.push('removeDocuments')
  }

  override async search(): Promise<RetrievalSearchPage> {
    this.calls.push('search')
    const hit: RetrievalHit = { id: 'a', scope: 'workspace', text: 'text', score: 1 }
    return { items: [hit] }
  }

  override async stats(): Promise<RetrievalStats> {
    this.calls.push('stats')
    return { workspaces: 0, documents: 0 }
  }

  override async flush(): Promise<void> {
    this.calls.push('flush')
  }
}

describe('retrieval service contract', () => {
  it('mounts one service and exposes the default close behavior', async () => {
    const ctx = new Context()
    const service = await ctx.plugin(StubRetrievalService)
    expect(ctx.retrieval).toBeInstanceOf(RetrievalService)
    const close = vi.spyOn(ctx.retrieval, 'close')
    await ctx.retrieval.close()
    expect(close).toHaveBeenCalledOnce()
    await service.dispose()
    expect(ctx.get('retrieval')).toBeUndefined()
  })

  it('builds typed failures with a stable code and cause', () => {
    const cause = new Error('native failure')
    const error = new RetrievalError('retrieval failed', 'RETRIEVAL_PROVIDER_FAILED', { cause })
    expect(error).toBeInstanceOf(Error)
    expect(error.name).toBe('RetrievalError')
    expect(error.code).toBe('RETRIEVAL_PROVIDER_FAILED')
    expect(error.cause).toBe(cause)
    const minimal = new RetrievalError('invalid request', 'RETRIEVAL_INVALID_REQUEST')
    expect(minimal.cause).toBeUndefined()
  })
})
