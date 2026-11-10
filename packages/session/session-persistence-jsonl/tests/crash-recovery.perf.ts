/**
 * Threshold-free built-runtime measurements of crash recovery in the JSONL
 * provider: a valid log whose physical tail is torn (truncated before the next
 * append) plus an abandoned materialization temporary (swept at write open),
 * against a clean reopen. Semantic interrupted-turn closers are synthesized by
 * the Agent loop and are not part of this measurement. Fixtures are synthesized;
 * each sample runs in a fresh process under plain Node. Diagnostic only — not
 * a `test:bench` gate.
 */

import { appendFile, mkdtemp, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { Context } from '@deepseek-ai/cordis'
import { SESSION_FORMAT_VERSION, SessionId, SessionSeq } from '@deepseek-ai/dsh-session'
import type { SessionEvent, SessionHeader } from '@deepseek-ai/dsh-session'
import * as JsonlSessionPersistence from '../src/index.ts'
import { logPath } from '../src/format.ts'

/** Completed turns in each synthesized log; the recovery log adds one open turn. */
const SIZES = [2_000, 20_000] as const
/** Fresh processes per size and scenario. */
const SAMPLES = 3
/** Abandoned materialization temporary written beside each recovery log. */
const STALE_TEMP = '.0123456789ab.tmp'

/** Build `turns` complete turn/start+turn/end pairs. */
function eventsFor(turns: number): SessionEvent[] {
  const events: SessionEvent[] = []
  let seq = 0
  for (let turn = 1; turn <= turns; turn++) {
    events.push({ type: 'turn/start', seq: SessionSeq(seq++), time: turn, data: { turn } })
    events.push({ type: 'turn/end', seq: SessionSeq(seq++), time: turn, data: { turn, reason: { kind: 'completed' } } })
  }
  return events
}

/** Write one valid fixture log through the production write path, then tear its physical tail. */
async function fixture(
  ctx: Context, root: string, turns: number, recovery: boolean,
): Promise<{ path: string; id: SessionId; nextSeq: number }> {
  const id = SessionId(recovery ? 'recovery' : 'balanced')
  const header: SessionHeader = {
    version: SESSION_FORMAT_VERSION,
    id,
    createdAt: 1,
    cwd: '/perf',
    isSeeded: false,
    delegationDepth: 0,
  }
  const events = eventsFor(turns)
  const handle = await ctx.sessionPersistence.create(header)
  await handle.append(events)
  await handle.close()
  const path = logPath(root, '/perf', id, 'none')
  if (recovery) {
    // A torn final record plus an abandoned materialization temporary: the next
    // write open must sweep the temporary and the first mutation must truncate
    // the incomplete record before appending.
    await appendFile(path, '{"type":"turn/end","seq":')
    await writeFile(`${path}${STALE_TEMP}`, 'abandoned'.repeat(128))
  }
  return { path, id, nextSeq: events.length }
}

/** Measure one size and scenario in this process. */
async function measure(turns: number, recovery: boolean): Promise<Record<string, number | string | boolean>> {
  const root = await mkdtemp(join(tmpdir(), 'dsh-recovery-perf-'))
  const ctx = new Context()
  try {
    await ctx.plugin(JsonlSessionPersistence, { root, compression: 'none' })
    // Fixture creation uses the same write path and is outside every interval.
    const { path, id, nextSeq } = await fixture(ctx, root, turns, recovery)
    const startOpen = performance.now()
    const handle = await ctx.sessionPersistence.open(id, 'write')
    const openMs = performance.now() - startOpen
    const startRepair = performance.now()
    const nextTurn = turns + 1
    await handle.append([
      { type: 'turn/start', seq: SessionSeq(nextSeq), time: 10_000, data: { turn: nextTurn } },
      { type: 'turn/end', seq: SessionSeq(nextSeq + 1), time: 10_000, data: { turn: nextTurn, reason: { kind: 'completed' } } },
    ])
    await handle.close()
    const repairMs = performance.now() - startRepair
    const startReopen = performance.now()
    const reader = await ctx.sessionPersistence.open(id, 'read')
    await reader.read()
    await reader.close()
    const reopenMs = performance.now() - startReopen
    const staleSwept = await stat(`${path}${STALE_TEMP}`).then(() => false, () => true)
    return { turns, recovery, openMs, repairMs, reopenMs, staleSwept }
  } finally {
    await ctx.fiber.dispose()
    await rm(root, { recursive: true, force: true })
  }
}

if (process.argv[2] === '--sample') {
  process.stdout.write(JSON.stringify(await measure(Number(process.argv[3]), process.argv[4] === 'recovery')) + '\n')
} else {
  const samples: Record<string, number | string | boolean>[] = []
  for (const turns of SIZES) {
    for (const recovery of [false, true]) {
      for (let sample = 0; sample < SAMPLES; sample++) {
        const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--sample', String(turns), recovery ? 'recovery' : 'balanced'], {
          encoding: 'utf8', timeout: 120_000,
        })
        if (result.error !== undefined) throw result.error
        if (result.signal !== null || result.status !== 0) {
          throw new Error(`measurement failed: ${result.signal ?? result.status}: ${result.stderr}`)
        }
        samples.push(JSON.parse(result.stdout) as Record<string, number | string | boolean>)
      }
    }
  }
  process.stdout.write(JSON.stringify({ node: process.version, platform: process.platform, arch: process.arch, samples }, null, 2) + '\n')
}
