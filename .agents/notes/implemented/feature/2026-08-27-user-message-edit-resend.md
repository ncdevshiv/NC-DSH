# Agent Note: User-message edit-and-resend

Status: implemented

English | [中文](2026-08-27-user-message-edit-resend.zh.md)

## Problem

A sent user bubble offered only copy, and the 2026-07-31 edit-stub note documented that editing must return together with the capability behind it: a client mutation over a settled user message, plus host behavior for the turn that already consumed it. Branching forward from the tail kept the wrong turn, so there was no gesture for "fix what I asked and rerun from there".

## Decision

Edit-and-resend is a backward fork, not a log edit:

- **Host `session.fork` gains `beforeSeq`** (mutually exclusive with `atSeq`). It inverts the anchor: the child inherits everything through the event before the turn containing the anchor, so the anchor's whole turn and every later event stay with the parent. An anchor whose turn never completed, or that lies outside every turn, fails with `fork-unavailable`. The cut reuses the anchored path's trailing-appends rule, so standalone events between the last kept turn and the cut still seed the child.
- **Client forwarding** — `ISessions.fork` and `SessionManager` accept and floor `beforeSeq` like `atSeq`.
- **UI** — user bubbles that open a completed turn get an edit action (steers and open-turn openers exclude it, mirroring the fork `OPEN_TURN` rule). Clicking opens a rewind dialog stating how many later turns stay in the original session; confirming forks with `beforeSeq`, opens the child, and restores the original text into the child composer via the input shell's draft write path. Nothing is auto-sent: the user edits and sends normally, matching the Codex desktop semantics.

No session-log format or session event is added — the append-only invariant holds because the parent session is untouched; only new locales and one RPC parameter moved.

## Alternatives considered

**Truncate the log in place.** Rejected: history is append-only by construction, and the fork approach keeps the original branch selectable.

**Auto-send the edited prompt after the fork.** Codex restores the prompt into the composer instead; auto-sending removes the last look the user is asking for.

**Invert the action toward file reversion.** Rewinding now reverts workspace files through the logged `write`/`edit` bases ([rewind workspace restore](2026-08-29-rewind-workspace-restore.md)); basis-less shell and `str_replace_editor` mutations are reported, not reverted.

## Consequences

The original session remains intact and selectable; the child carries `parentSession` lineage. Only text content is restored into the composer — image attachments and non-text content of the edited message are not carried, which the dialog does not advertise. The 2026-07-31 stub-drop note is superseded by this change; the branch action stays assistant-only ([user-bubbles-drop-the-branch-action](../simplification/2026-08-06-user-bubbles-drop-the-branch-action.md) remains in force for branching).
