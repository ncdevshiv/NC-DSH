# Agent Note: Self-modification closed loop — staged capability program

Status: proposed

English | [中文](2026-08-25-self-modification-closed-loop.zh.md)

## Problem

The commissioned objective: DSH should know its own crashes and errors; know its users (persona, behavior, frustration) and their projects and problems; compile realtime self-reflection into concrete modification proposals; live-edit its own composition without crashing; and ship replacements through spawn → test → cutover → retire. Today only fragments exist, verified by the capability audit this note cites: model-written plugins are session-scoped and vanish on restart while built-ins are untouchable; the invariant service ships unmounted and telemetry defaults to DISABLED with best-effort delivery that loses records exactly when a crash makes them matter; user feedback, approvals, and titles are captured durably but never joined across sessions and are invisible to search extraction; no reflection loop turns observations into proposals; cutover of self-authored packages has version mechanics but no test-before-promotion, no automatic rollback, and no survival across restart.

## Proposal

Execute five independently valuable phases, ordered so the system becomes honest about itself before it rewrites itself; [SELF-MODIFICATION-AUDIT.md](../../../../SELF-MODIFICATION-AUDIT.md) owns the evidence and package-level detail:

1. See crashes — mount `dsh-invariants` in shipped bundles; add an `uncaughtException`/rejection crash reporter persisting a durable crash record; implement the deferred telemetry outbox.
2. Join user truth — widen `extractSessionEventText` to feedback/approval/title events; index the message-feedback domain into the session-query read model; add a `user-model` storage-domain joining anonymous-id → workspace → sessions → signals.
3. Reflect — a retrospective capability consuming session-query + telemetry summaries after failures, emitting reviewable proposals (candidate Agent Notes, preset patches, dynamic-package drafts).
4. Persist self-edits — storage-backed dynamic packages replayed on boot with remembered approvals; an explicit security stance for host-only activations before reach broadens.
5. Canary cutover — a runner mode that exercises a candidate against assertions and promotes only on pass, wired to automatic rollback via the kept-pointer mechanism.

## Alternatives considered

**Build one monolithic meta-agent beside the harness.** Rejected: it fights the capability-seam architecture, duplicates session-query and telemetry, and concentrates risk in one plugin instead of five auditable phases.

**Adopt a cloud analytics/SaaS pipeline first.** Rejected: durability and privacy come first — a durable outbox must exist before anything leaves the machine, and frustration modeling aggregates locally by default.

**Defer until the need hurts.** Rejected: the objective is explicitly commissioned, and phases 4–5 without phase 1 would automate existing blind spots at higher speed.

## Acceptance criteria

Phase 1: every shipped bundle mounts invariants; killing the host mid-turn leaves a readable crash record on disk; queued telemetry survives the kill. Phase 2: `/feedback` text and negative ratings are searchable cross-session; a query returns per-workspace problem timelines. Phase 3: after a failed turn, a generated retrospective names causes and files concrete proposals for human review. Phase 4: a defined dynamic plugin survives restart with its grants; host-only activation policy is written down and enforced by composition tests. Phase 5: a failing candidate package is promoted never and rolled back automatically; a passing one takes over on restart.

## Risks

Host-only activations currently run with zero approval (`cordis-host-runner/src/index.ts:270-275`) — persistence and broader reach multiply that exposure unless the security stance lands first. Frustration/user modeling is sensitive data; aggregation stays local and derived from already-durable events. Restart-persistent self-edits become an escalation vector if approval memory outlives consent — grant expiry belongs in phase 4's design. Scope pressure against the pre-release stance is bounded by phase independence: each phase ships value alone and can pause without stranding work.
