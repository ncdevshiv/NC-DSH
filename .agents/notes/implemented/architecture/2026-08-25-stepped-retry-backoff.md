# Agent Note: Stepped retry backoff — one flat delay per tier of consecutive retries

Status: implemented

English | [中文](2026-08-25-stepped-retry-backoff.zh.md)

## Problem

The default provider retry policy used bounded exponential backoff (500 ms doubling every retry to a 10-second cap, ±10 percent jitter). A provider route in always mode reached the 8-second plateau on its fifth retry and paid roughly 7–8 seconds for every later attempt, even when each failure was an isolated crash and the model itself answered quickly. With model latency no longer the dominant cost, the locally scheduled wait was what made sessions feel stuck.

## Decision

**The delay is held flat across a tier of consecutive retries and doubles only between tiers.** `BackoffConfig` gains `doubleEveryRetries` (default 10): the delay before retry *n* is `initialDelayMs * 2^floor((n - 1) / doubleEveryRetries)`, still clamped by `maxDelayMs` and jittered symmetrically. The defaults move from 500 ms to 2000 ms, so the out-of-box ladder is ten consecutive retries at about 2 seconds, then ten at 4 seconds, then 8 seconds until the 10-second cap absorbs further tiers. `doubleEveryRetries: 1` restores the previous per-retry doubling. The resolved-policy key gains the new field, so a route replacement that changes it starts its own retry history; a provider `Retry-After` at or below `maxDelayMs` continues to override local backoff verbatim.

## Consequences

A single transient crash now waits about 2 seconds before the first retry instead of 500 ms — slightly slower recovery from one-off blips is the accepted price. In exchange, sustained-failure chains stop racing to the cap: the wait stays at ~2 seconds through ten attempts, and only a genuinely persistent outage escalates toward 4, then 8 seconds. Recorded snapshots whose `llm/retry` events embed the policy key were updated mechanically for the appended field.

## Alternatives considered

- **Lower `initialDelayMs` under the old curve** — the escalation to the plateau is the problem; shrinking the base only delays it.
- **Drop local backoff and honor only provider `Retry-After`** — most transport and empty-response failures carry no header, so the executor still needs a local ladder.
- **Remove jitter alongside flattening** — jitter protects against synchronized retry storms across concurrent agents and subagents sharing one provider route.
