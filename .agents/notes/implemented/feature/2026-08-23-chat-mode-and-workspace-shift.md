# Agent Note: Chat Mode and the Workspace Shift

Status: implemented

English | [中文](2026-08-23-chat-mode-and-workspace-shift.zh.md)

## Problem

The web client gated all conversation behind Workspace selection: with no Workspace registered, startup selection stayed idle, the resident composer rendered inert with a "Choose workspace" placeholder, and New Session cleared into that same dead-end state. A user could not send a single message until they picked or created a directory. The Host already supported Workspace-less sessions (`session.create` with no `workspaceId`/`cwd` defaults to the Host cwd), but no UI path ever created one.

Separately, a session's cwd is immutable once created (`SessionCwdConflict`; both persistence backends store logs under cwd-derived locations), so a conversation started without a Workspace can never be *moved* into one. Picking a workspace mid-conversation therefore had to choose between discarding the conversation (the existing blank-session reuse) and keeping it unanchored.

## Decision

**Chat mode is a first-class state, not an error posture.** A workspace-less session is fully usable: the composer stays editable, and its chip reads the localized chat label ("聊天"/"Chat") with the new-chat glyph while still opening the Workspace picker — the picker is how chat shifts into work mode. The inert read-only textarea now covers only the truly-no-session state (boot before any baseline).

**Chat bootstrap reuses the blank-reuse machinery.** `WorkspaceRuntime.connectChat()` mirrors `connectWorkspace()`: reuse an ungrouped non-archived blank session from the list mirror, else create one with no Workspace on the Host; concurrent callers coalesce through the same in-flight map under a synthetic key. With zero Workspaces registered, both `startSession()` and the one-shot startup selection land in the chat session instead of clearing. Chat sessions surface in the sidebar's existing Ungrouped bucket once non-blank; blanks stay hidden as always.

**The shift is a retargeted fork, not a move.** `session.fork` gains an optional `workspaceId`: the child adopts the named Workspace's path as its own cwd and joins that Workspace instead of following the source (or a subagent's nearest owning ancestor), so the seeded history is retained verbatim while every later turn runs — and groups — inside the target. The seed still references the source cwd; retention means context, not relocation. An unknown id fails with `workspace-not-found`; a post-publication attachment failure keeps the established `workspace-attach-failed` partial-success contract. The wire request carries only `workspaceId`; the client passes the target path as a display-only `cwd` hint for the optimistic child row. Blank sessions keep the cheap reuse-or-create path with draft carry-over; only non-blank sessions fork.

## Alternatives considered

- **Rewriting the session header cwd on attach**: headers are immutable and JSONL logs physically live under cwd-derived project directories; a move would require log migration for no durable benefit.
- **Seeding the new session with a synthetic context message**: changes model-visible content without a real turn, and duplicates history the fork mechanism already carries exactly.
- **Auto-attaching by matching Host cwd**: the registry deliberately requires index membership plus canonical cwd match; loosening it would silently group CLI-born sessions.

## Consequences

- A fresh deployment can be used as pure chat with zero configuration; adding a workspace later adopts ongoing conversations via the picker.
- The deleted-workspace hero case (previously forced back to "Choose workspace") now degrades to chat mode — the session remains usable, which is strictly better than a wall.
- Ungrouped non-blank sessions accumulate if users repeatedly shift chats; the sidebar's Ungrouped bucket and archive are the existing answers.
