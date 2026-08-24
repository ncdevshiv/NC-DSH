# Agent Note: Surface New Session connect outcomes and bound session.create on the wire

Status: implemented

English | [中文](2026-08-24-new-session-connect-outcome-surfacing.zh.md)

## Problem

Every New Session trigger was fire-and-forget: `startSession` returned `void`, both shell adapters discarded the call, and connect failures were reduced to `console.warn`. A connect can legitimately take minutes — the Host serves every session's turns, folds, and compression from one process, and `session.create` had no deadline on either side of the wire — so under load a click looked like a dead button with no pending state, and a rejected create looked like nothing at all. Because the host persists the session header before preset mount completes (`api-proxy.ts` resolves the id pre-setup), an abandoned click still left a permanent header-only blank session on disk. Recovery was accidental: when the load burst ended, the queued create landed and a blank row silently appeared. Related mechanism notes: [Workspace UI product flow](../feature/2026-07-25-workspace-ui-product-flow.md), [chat mode](../feature/2026-08-23-chat-mode-and-workspace-shift.md), [blank-reuse membership](2026-08-05-workspace-blank-session-reuse-membership.md).

## Decision

- `IWorkspaces.startSession` returns `Promise<SessionId>`; it resolves after navigation and draft reset, and rejects — after logging — carrying the business reason. Programmatic callers that want the old fire-and-forget shape opt in explicitly with `.catch(() => {})` (the agent-preset creator draft).
- Each trigger surface renders outcome feedback at the control: a busy label plus re-entry guard, and a rejected connect as a `role="alert"` line beside the trigger. The guard is shared per surface because New Session mints fresh sessions (`forceNew`), so concurrent clicks would create duplicate blanks; the runtime coalescing does not apply on that path.
- `SessionManager.create` settles its caller at `SESSION_CREATE_TIMEOUT_MS` (30s, the UX scale of `streamOpenTimeoutMs`) with error code `session-create-timeout`. A late settlement still merges the published blank session into the list, so an eventually-created session never stays invisible.
- `scripts/dev-desktop.mjs` tees each child's stdout/stderr lines to `<DSH_HOME>/logs/<label>-<timestamp>.log`; previously the only copy of host-side diagnostics died with the launcher console, leaving failures like this one untraceable after the fact.

## Alternatives considered

**A global toast service.** Rejected: no such channel exists in the client stack, and inventing one for two surfaces exceeds the need; the per-trigger `role="alert"` line matches the established local-error idiom (rename dialogs, settings sections).

**Publishing connect failures into `WorkspaceListState.error`.** Rejected: that field is the list-pull state axis; an action outcome is request-local and would linger after the next successful pull.

**A host-side create deadline.** Rejected here: a slow-but-succeeding create is normal under load, and aborting server-side would strand partially mounted agents. Bounding the caller while letting the settlement merge keeps client patience and host truth independent.

## Consequences

- Slow or failed connects are visible at the exact control the user pressed, in both sidebar widths and in the browser region; repeated clicks during a pending connect can no longer mint duplicate blank sessions.
- A timed-out click may still produce a session row afterwards (the late merge). This is intentional: the session exists on the host, and hiding it would recreate the original bug in the opposite direction.
- The 30s timeout is a fixed UX constant, not configuration; if deployments ever need a different value it should become runtime configuration together with the connection-layer timeouts.
- Verification: `manager.client.spec.ts` (timeout result, late merge), `workspaces-service.client.spec.ts` (rejection propagates, no open), `sidebar-root.client.spec.tsx` (guard, busy label, alert, recovery), `workspace-browser.client.spec.tsx` (inline alert), plus the touched packages' full suites.
