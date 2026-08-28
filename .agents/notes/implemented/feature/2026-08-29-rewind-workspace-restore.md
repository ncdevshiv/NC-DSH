# Agent Note: Rewind workspace restore

Status: implemented

English | [中文](2026-08-29-rewind-workspace-restore.zh.md)

## Problem

Edit-and-resend (the 2026-08-27 user-message note) rewound the conversation to before the edited turn, but the workspace kept the discarded turns' file changes — the parent thread's state, not the child's. Codex's equivalent feature explicitly does not revert files (it delegates to git); our design goals for the checkpoint plan were stronger: when the user rewinds, the workspace should match the splice.

## Decision

A new policy package, `dsh-turn-restore`, provides `ctx.turnRestore` (zero-config function plugin, optional composition like `session-checkpoint-policy`). `session.fork` with `beforeSeq` consults it BEFORE publishing the child and inverse-replays the discarded events:

- **The journal is the log itself.** The `write`/`edit` tools already attach their applied-hunk diffs to `tool/result.meta`; they now also attach a full-text `basis` (same before/after the backend computed, plus the `create`/`update`/`edit` op). Tool/result meta is the tool-private opaque, JSON-validated slot — no new session event, no SDK surface change, replay-safe by construction.
- **Restore is observationally safe.** A basis is applied newest-first and only when the current disk content still equals the basis's post-write text; an intervening user edit turns that entry into a reported conflict rather than a clobber. Create files whose content matches are deleted; a create file already missing counts as pre-state.
- **Limit honesty, not silent gaps.** Basis-less mutations are reported: `str_replace_editor` and size-capped writes (`before: null` on update) count as `notRestorable`, `bash`/`pwsh`/`terminal` count as `shell` with names. A restore never runs while the source agent is still running, nor without a session cwd; both skip with a `skipped` reason.
- **The client surfaces the pass.** `session.fork`'s `beforeSeq` response gains an optional `restoreReport` (summaries on the host contract, mirrored on the client contract); the edit-and-resend flow renders it as info/error notice on the child's composer.

## Alternatives considered

**Per-turn filesystem snapshots.** Full-tree snapshots restore files no tool can prove it wrote and double storage; the log-driven basis approach restores exactly what the removed turns did and nothing else.

**A separate journal store.** Would need its own durability, replay, and pruning rules; the tool meta already carries the facts and is already durable and replayable.

**Restoring while the source agent runs.** Rejected as a race: the restored state could be overwritten by the still-live branch. Skipping with a reason keeps the child consistent and lets the user stop the source and rewind again.

## Consequences

A rewind now restores all `write`/`edit` file effects of the removed turns (bounded by the fs-local diff basis cap) and reports everything it could not revert. Bash side effects remain un-reverted (nothing can roll back an arbitrary shell run); the report names the shell tools that ran. The parent branch keeps its own log and, if running, its own file work — the shared workspace after a busy-source skip belongs to whatever the parent does next.
