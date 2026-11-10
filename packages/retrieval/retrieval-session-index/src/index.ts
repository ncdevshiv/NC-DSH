/**
 * Live session-event indexing into the workspace retrieval service.
 *
 * Every committed event whose semantic text is non-empty upserts into the
 * active session's workspace collection. Disposal drains the per-session
 * write chains so a committed event never outlives its owning plugin.
 *
 * @module @deepseek-ai/dsh-retrieval-session-index
 */

import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type { Session, SessionEvent, SessionId } from '@deepseek-ai/dsh-session'
import { buildSessionEventSearchDocuments } from '@deepseek-ai/dsh-session-query'
import type { RetrievalDocument } from '@deepseek-ai/dsh-retrieval'

/** Cordis plugin name used by Loader diagnostics. */
export const name = 'retrieval-session-index'

/** Services required to mirror live session events into retrieval. */
export const inject = ['retrieval', 'sessions']

/** Deployment-owned indexing bounds. */
export interface Config {
  /** Maximum semantic text bytes retained for one event. Defaults to 8192. */
  maxTextBytes?: number
}

/** Schemastery config for Loader defaults and generated configuration docs. */
export const Config: z<Config> = z.object({
  maxTextBytes: z.number().step(1).min(1).max(1_048_576).default(8192),
})

interface ResolvedConfig {
  readonly maxTextBytes: number
}

/** Index live session events into their workspace retrieval collection. */
export function apply(ctx: Context, config: Config): void {
  /* v8 ignore next -- Loader's Config schema fills the default before apply */
  const resolved: ResolvedConfig = { maxTextBytes: config.maxTextBytes ?? 8192 }
  const tails = new WeakMap<Session, Promise<void>>()
  const dispose = async (): Promise<void> => {
    for (const tail of pendingTails) await tail
    pendingTails.clear()
  }
  const pendingTails = new Set<Promise<void>>()
  ctx.effect(() => dispose, 'retrievalSessionIndex.drain')

  ctx.on('session/event', (session, event) => {
    const workspaceRoot = session.header.cwd
    if (workspaceRoot === undefined) return
    const documents = documentsFor(session, event, resolved.maxTextBytes)
    if (documents.length === 0) return
    const prior = tails.get(session) ?? Promise.resolve()
    const next = prior.then(async () => {
      await ctx.retrieval.indexDocuments({ workspaceRoot, documents })
    }).catch((error: unknown) => {
      ctx.logger.warn(
        /* v8 ignore next -- only the retrieval provider rejects, and it rejects Error instances */
        `retrieval-session-index: failed to index session ${session.id}: ${error instanceof Error ? error.message : String(error)}`,
      )
    })
    pendingTails.add(next)
    tails.set(session, next)
    void next.finally(() => pendingTails.delete(next))
  })
}

function documentsFor(
  session: Session,
  event: SessionEvent,
  maxTextBytes: number,
): RetrievalDocument[] {
  const documents = buildSessionEventSearchDocuments(session.id, [event])
  return documents.map((document) => {
    const text = truncateBytes(document.text, maxTextBytes)
    return {
      id: documentId(session.id, document.seq),
      scope: 'session',
      text,
      sessionId: session.id,
      seq: document.seq,
      type: document.type,
      surface: document.surface,
      time: document.time,
    }
  })
}

function documentId(sessionId: SessionId, seq: number): string {
  return `session:${sessionId}:${String(seq)}`
}

function truncateBytes(text: string, maxBytes: number): string {
  if (Buffer.byteLength(text, 'utf8') <= maxBytes) return text
  const characters = Array.from(text)
  let end = characters.length
  while (end > 0 && Buffer.byteLength(characters.slice(0, end).join(''), 'utf8') > maxBytes) end -= 1
  return characters.slice(0, end).join('')
}
