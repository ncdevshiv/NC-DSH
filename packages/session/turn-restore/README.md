# dsh-turn-restore

English | [中文](README.zh.md)

Rewind-time workspace restore for edit-and-resend forks. A `session.fork` rewind (`beforeSeq`) discards whole turns; this policy provides `ctx.turnRestore`, which inverse-replays those discarded turns' logged `write`/`edit` tool bases against the session workspace before the child is published, so the workspace matches the conversation splice instead of trailing the removed work.

## Plugin (namespace: `turn-restore`)

Zero-config function plugin; the host's `session.fork` reads it through `ctx.get('turnRestore')`, so an app without this plugin composed rewinds the conversation only:

```yaml
- id: turn-restore
  name: '@deepseek-ai/dsh-turn-restore'
```

The restore pass is driven entirely by session-log facts — the full-text restore basis the `write`/`edit` tools attach to `tool/result` meta (see `dsh-tool-fs`) — so it replays identically on live and persisted sources, no separate journal exists. For each basis (newest first) the policy rewrites the file to its pre-write text only when the current disk content still equals the basis's post-write text; a divergence (an intervening user edit) turns into a reported conflict instead of a clobber.

## Model Experience

### Summaries

#### What the model sees

The policy adds no prompt, tool schema, or session event. Rewinds happen between turns, never inside one, so no model request observes an intermediate file state.

#### Token effect

No change.

#### KV Cache effect

No change.

## Known Limitations and Deferred Work

- Only `write` and `edit` carry a restore basis. `str_replace_editor`, `bash`, `pwsh`, and `terminal` mutations are counted (`notRestorable` / `shell`) and reported, not reverted.
- A `write` over the fs-local `diffBasisMaxBytes` cap records no before-text (`before: null` on an update) and is therefore un-restorable; the report counts it.
- The restore is skipped (`skipped: 'source-running'`) while the source agent is running and skipped (`skipped: 'no-cwd'`) for workspace-less chat sessions; the rewind itself still proceeds in both cases.
- Line endings: the basis is LF-normalized, so a CRLF file restored from an LF basis rewrites with LF endings.
- Deferred: inverse restoration of `str_replace_editor` (same meta shape) and per-turn git-style checkpoints.
