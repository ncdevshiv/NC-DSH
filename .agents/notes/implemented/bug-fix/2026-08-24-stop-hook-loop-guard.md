# Agent Note: Stop-hook loop guard

Status: implemented

English | [中文](2026-08-24-stop-hook-loop-guard.zh.md)

## Problem

Both hook bridges mapped a blocking Stop outcome to `agent.steer()` at `agent/turn-stopping`, and every Stop payload reported `stop_hook_active: false`. An unconditionally blocking hook therefore re-armed continuation at every stopping boundary: each block forced a full model request, and nothing bounded the count. The only intended defense was hook authors self-limiting off a flag the bridges never set true, so a misbehaving `hooks.json` meant unbounded spend per turn.

## Decision

Each bridge keeps a `StopBlockLedger` (`dsh-hook-protocol`) keyed weakly by the live agent and the open turn number. Reading the ledger before running the hooks is what makes `stop_hook_active` honest: `false` on a turn's first boundary, `true` once that turn already followed a forced continuation. On a merged deny with the budget spent, the bridge logs a warning naming `maxConsecutiveStopBlocks` (positive-integer validated config, protocol default `DEFAULT_MAX_CONSECUTIVE_STOP_BLOCKS` = 25) and lets the boundary commit; below the cap it records one block and steers as before.

The count covers the whole continuation chain since the turn first tried to stop, not strictly uninterrupted blocks. Pinning: the protocol package's unit spec runs everywhere and pins ledger semantics; each bridge boots the real loop with an always-blocking hook and asserts the capped request count, the `[false, true, …]` payload-flag sequence, and cross-turn budget freshness.

## Alternatives considered

- **Strictly consecutive counting** — rejected: a hook alternating block/allow never exceeds an uninterrupted count of one while still forcing a request per cycle.
- **A `session/event` `turn/end` listener resetting per-session counters** — rejected: an extra subscription plus cleanup to derive a fact the turn number already carries; the turn-scoped ledger key resets budgets with no listener, and WeakMap keys die with the agent instance.
- **Relying on `stop_hook_active` alone** (the reference implementations' model) — rejected here: unmodified third-party hooks ignore the flag; only a hard cap bounds spend when they do not self-limit.
- **Honoring `{"continue": false}` as a run-level halt** — the separate `TODO(hook-continue-false)` control; this change deliberately scopes to the deny→steer path.

## Consequences

A blocking Stop hook extends a turn by at most `maxConsecutiveStopBlocks` continuations; further blocks log and close the turn, and each new turn starts with a fresh budget. Self-limiting hooks behave exactly as before, now with a truthful `stop_hook_active` matching the reference wire semantics. SubagentStop payloads still report `false` — that point remains observe-only.
