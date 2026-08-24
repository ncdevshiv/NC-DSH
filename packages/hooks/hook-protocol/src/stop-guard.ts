/**
 * Per-agent ledger of blocking Stop-hook outcomes within one open turn, shared
 * by the Claude Code and Codex bridges. A turn's continuation chain starts at
 * its first blocking Stop boundary: every later boundary in the same turn
 * counts, including boundaries that follow a non-blocking one, because a hook
 * that alternates block/allow would otherwise stay under a strictly
 * uninterrupted count forever. The count resets when the agent opens its next
 * turn, so each turn gets a fresh budget.
 * @module @deepseek-ai/dsh-hook-protocol/stop-guard
 */

/**
 * Default {@link StopBlockLedger} budget when a bridge config omits
 * `maxConsecutiveStopBlocks`. Generous enough for legitimate fix-and-retry
 * loops, tight enough that a misbehaving always-blocking hook cannot extend a
 * turn indefinitely.
 */
export const DEFAULT_MAX_CONSECUTIVE_STOP_BLOCKS = 25

/** Blocks recorded for one agent's open turn. */
interface TurnChain {
  turn: number
  blocks: number
}

/** Tracks consecutive Stop-hook blocks so a bridge can cap forced continuations. */
export interface StopBlockLedger {
  /**
   * Blocks already recorded for `agent`'s `turn`; `0` on the turn's first
   * Stop boundary. Reading before running the hooks is what makes the
   * `stop_hook_active` payload flag honest.
   * @param agent - the live agent at its stopping boundary (the ledger key;
   * entries die with the agent instance).
   * @param turn - the turn whose stopping boundary is firing.
   * @returns blocks previously recorded for this agent and turn.
   */
  blocks(agent: object, turn: number): number
  /**
   * Record one honored block: the bridge steered and the turn will run at
   * least one more step. Recording a past turn's number starts that turn's
   * chain fresh.
   * @param agent - the live agent the block forced forward.
   * @param turn - the turn the block extended.
   */
  recordBlock(agent: object, turn: number): void
}

/**
 * Create a {@link StopBlockLedger} (one per bridge `apply()`). Keyed weakly by
 * the agent instance, so disposed agents release their entry and a long-lived
 * server holds at most one small record per live agent.
 * @returns the ledger.
 */
export function createStopBlockLedger(): StopBlockLedger {
  const chains = new WeakMap<object, TurnChain>()
  const blocks = (agent: object, turn: number): number => {
    const chain = chains.get(agent)
    return chain !== undefined && chain.turn === turn ? chain.blocks : 0
  }
  return {
    blocks,
    recordBlock(agent: object, turn: number): void {
      chains.set(agent, { turn, blocks: blocks(agent, turn) + 1 })
    },
  }
}
