# Session Format Inventory (Phase 1 Evidence)

Inventoried on this machine (Windows, user home = `~`), 2026-08. This document is
the ground truth for every real parser in `agents/<name>/adapter.py`. Formats were
derived by reading actual files under the user profile — no invented paths.

General parsing contract for all adapters:

- Discovery is newest-first by file mtime and bounded by an `hours_back`
  parameter (default 168h) so indexing stays incremental.
- JSON Lines files are parsed **line-by-line tolerantly**: a line that fails
  `json.loads` is skipped and counted in a debug log; the rest of the file is
  still processed.
- Every parser emits dicts shaped `{"role", "content", "timestamp",
  "session_id"}` with non-empty string content only.
- Noise records (command echoes, auth errors, injected context blocks) are
  filtered per-format below.

---

## Claude Code (`agents/claude_code/adapter.py`)

- Path pattern: `~/.claude/projects/**/<session-uuid>.jsonl`
  - The projects directory contains one subdirectory per working directory,
    munged from the absolute path (e.g. `F--nc-code` for `F:\nc-code`).
    Session files sit directly inside those subdirectories.
  - Legacy fallback also probed: `~/.claude/sessions/*.json` (OpenMem's old
    assumed layout — kept for compatibility, not observed here).
- Encoding: UTF-8 JSON Lines. One record per line.
- Record types observed (counts from live inventory, 7 files / 85 records):
  `attachment`, `queue-operation`, `user`, `assistant`, `last-prompt`,
  `atis-latch`, `custom-title`, `system`, `mode`.
- Role extraction rule:
  - `type == "user"` → role `user`. Content at `message.content`, which may be
    a plain string OR an array of typed blocks. For arrays, concatenate the
    `text` of every `{"type": "text", "text": ...}` block; records carrying
    only `tool_result` / other non-text blocks are skipped.
  - `type == "assistant"` → role `assistant`. Same content rule over
    `message.content` (blocks look like `{"type": "text", "text": ...}`).
  - All other types are metadata/noise and are skipped.
- Timestamps: record-level ISO-8601 UTC field `timestamp`
  (e.g. `2026-08-21T15:02:24.681Z`). Session id: record-level `sessionId`.
- Corruption handling: malformed lines skipped + counted; unreadable files
  skipped with a warning.
- Noise filters applied:
  - Assistant records with `message.model == "<synthetic>"` — these carry API
    error text (observed: "Failed to authenticate. API Error: 401 ..."), not
    real conversation.
  - User records that are CLI command echoes: content starting with
    `<command-name>`, `<command-message>`, `<local-command-caveat>`,
    `<local-command-stdout>`, or `<system-reminder>`.
- Oddities future phases must know:
  - `queue-operation` records duplicate the user prompt that follows as a real
    `user` record — do NOT index them or prompts get double-counted.
  - Records carry `isSidechain` (subagent/side-chain transcripts interleaved in
    the same file). Sidechain records are excluded by default; flip when
    sidechain analysis becomes a feature.
  - `attachment` records embed large tool/agent listings; never treat them as
    conversation content.

## Codex CLI (`agents/codex_cli/adapter.py`)

- Path pattern: `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`
  (recursive date-partitioned tree).
- Record types observed (1 file / 13 records): `session_meta`,
  `response_item`, `event_msg`, `turn_context`, `world_state`.
- Layout: every record has top-level `timestamp`, `ordinal`, `type`, and a
  nested `payload`.
- Role extraction rule:
  - `type == "response_item"` AND `payload.type == "message"`:
    - `payload.role == "user"` → user; `payload.role == "assistant"` →
      assistant; `payload.role == "developer"` → skipped (system/instruction
      injection, not conversation).
    - Content: `payload.content` array of blocks with `{"type":
      "input_text"|"output_text", "text": ...}` — concatenate all block texts.
  - `type == "session_meta"` → source of the canonical session id
    (`payload.session_id`); falls back to the rollout filename stem.
  - Everything else is telemetry → skipped.
- Timestamps: top-level ISO-8601 UTC `timestamp` per record.
- Corruption handling: malformed lines skipped + counted; unreadable files
  skipped with a warning.
- Noise filters applied: user-role messages whose text is purely machine-
  injected XML context are dropped when they start with wrappers such as
  `<environment_context>`, `<recommended_plugins>`, `<skills_instructions>`,
  `<user_instructions>`, `<permissions_hint>`.
- Oddities: genuine-looking user turns may still be harness health-checks
  (observed: "Reply with exactly: OK"); they parse as ordinary messages.
  Codex also keeps sqlite state stores (`state_*.sqlite`,
  `thread_history_*.sqlite`) — out of scope this phase; the JSONL rollouts are
  the conversation source of truth.

## Cursor (`agents/cursor/adapter.py`)

- Observed on this machine: `~/.cursor/` exists but contains ONLY an empty
  `sessions/` directory — no chat transcripts available in any file format.
  Cursor's richer state lives in workspace `state.vscdb` sqlite databases,
  which are out of scope this phase (no new dependencies; schema unverified).
- Parser behaviour therefore: tolerant discovery of `~/.cursor/sessions/**/*.json`
  and workspace `.cursor/sessions/*.json`; returns [] when nothing exists.
- If Cursor history files appear later, inventory them before extending this
  parser — no speculative format support was written.

## OpenClaw (`agents/openclaw/adapter.py`) — pre-existing

- Path pattern: `~/.openclaw/workspace/sessions/*.json` plus
  `~/.openclaw/sessions/*.json`; single JSON document with a `messages`
  (or `conversation`) array of `{role|sender, content, timestamp}` objects.
- Not present on this machine today; kept as the reference adapter-driven path
  via the shared session-source machinery.

## Other adapters

`qwen_code`, `opencode`, `antigravity_ide`, `kilo_cli`, `vscode`,
`windsurf`, `generic`: no local history stores found on this machine to
inventory. They keep conservative discovery paths and return [] when absent;
do not fabricate formats for them without first finding real files.
