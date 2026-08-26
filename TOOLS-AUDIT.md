# DeepSeek Harness — Tools & Languages Audit

Scope: every model-facing tool mounted in the current agent session, the repository surfaces that implement them, the language each is written in, and a reasoned keep/change verdict per surface. All findings below were produced by direct inspection of this working tree (`git ls-files`, targeted greps of `src/index.ts` registrations, package manifests).

## 1. Executive summary

- **25 tools** are mounted in this session. **Every one is implemented in TypeScript** (ESM, `strict: true`) somewhere under `packages/`.
- Repository-wide, TypeScript is ~560k lines (2,482 `.ts` + 264 `.tsx` tracked files). Everything else is deliberately small: Python SDK (21 files, ~4.3k lines), one Rust file (363 lines, the Landlock launcher), 56 SQLite DDL files, plus website/build-script glue.
- **Verdict: change nothing.** Each non-TypeScript surface exists for a reason TypeScript cannot serve (kernel LSM access; a Python-speaking audience). The TypeScript surface should stay TypeScript because the architecture is a typed same-process plugin graph and the workload is I/O-bound, not CPU-bound. Concrete triggers that would justify revisiting are listed in §6.

## 2. Session tool inventory — what this agent actually has

| Tool | Implementing package(s) | Language | Backing process / API |
|---|---|---|---|
| `read`, `write`, `edit`, `read_image` | `packages/fs/tool-fs` (`read.ts`, `write.ts`, `edit.ts`, `read-image.ts`) | TypeScript | `dsh-fs` capability + policy (`fs-local`, `fs-sandbox`) |
| `glob`, `grep` | `packages/fs/tool-fs-search` (`glob.ts`, `grep.ts`) | TypeScript | filesystem scan |
| `pwsh` | `packages/shell/tool-pwsh`; providers `pwsh-local` / `pwsh-sandbox` | TypeScript | spawns `pwsh.exe` / `bash` (external interpreters); background jobs via `dsh-jobs` |
| `web_search` | `packages/web/tool-web`; providers `web-search-{deepseek,exa,perplexity,searxng}`, `web-fetch-http`, `web-fetch-moli` | TypeScript | outbound HTTPS to search providers |
| `ask_user_question` | `packages/interaction/tool-ask-user` | TypeScript | interaction/approval services |
| `todo_write` | `packages/todo/tool-todo` | TypeScript | session log |
| `skill` | `packages/skill/tool-skill` + `skill-filesystem`, `skill-badge` | TypeScript | skill catalog on disk |
| `subagent` (delegate), `send_message`, `interrupt_agent`, `list_agents` | `packages/subagent/tool-subagent` (configurable `toolName`), `tool-subagent-control` (`list-agents.ts`) | TypeScript | providers: `subagent-spawn-in-process`, `subagent-fork-in-process`, `subagent-dsh-sdk`, `subagent-acp`, `subagent-claude-code`, `subagent-codex` |
| `subagent_fork` | fork provider behind the same delegation seam (`subagent-fork-in-process`) | TypeScript | in-process conversation fork |
| `workflow` | `packages/workflow/tool-workflow` + `workflow-worker-thread` | TypeScript | `node:worker_threads` worker executes the orchestration script |
| `ralph` | `packages/workflow/tool-ralph` | TypeScript | fresh-agent loop driver |
| `create_goal`, `get_goal`, `update_goal` | `packages/goal/tool-goal` (+ `goal`, `goal-round-driver`) | TypeScript | persisted goal state |
| `job_output`, `job_list`, `job_kill` | `packages/jobs/tool-jobs` + `jobs-local` | TypeScript | background job registry |
| `exit_plan_mode` | `packages/plan/plan-mode` (`EXIT_PLAN_MODE` constant; plan tool registered alongside) | TypeScript | logged plan state |

Adjacent capability in this session: **8 skills** (Markdown instruction packs loaded via `skill`), not code tools.

## 3. Implemented in the repo but not mounted in this session

For completeness — these are real capability seams today, all TypeScript:

- `bash` / persistent shells: `packages/shell/tool-bash*`, `terminal/tool-terminal`
- Browser automation: `packages/browser/{browser,browser-moli,tool-browser}` (CDP)
- LSP: `packages/lsp/{lsp,lsp-stdio,tool-lsp}`
- Sandboxed code execution: `packages/code-runtime/*` (worker-thread and Python runtimes; `py/protocol.py`)
- MCP client: `packages/mcp/mcp-client`
- Session query/export: `packages/session-query/*` incl. `tool-session-query`
- String-replace editor variant: `packages/fs/tool-str-replace-editor`
- E2B cloud sandbox adapters: `packages/e2b/*`
- Automation servers/SDKs: `packages/acp`, `packages/sdk` (JSON-RPC protocol/server/client)

## 4. Language census (tracked files, this tree)

| Language | Files | Lines | Where | Role |
|---|---|---|---|---|
| TypeScript (`.ts`) | 2,482 | 495,230 | `packages/`, `apps/`, `scripts/`, `vendor/` | all tools, plugins, agent loop, CLI, servers |
| TypeScript UI (`.tsx`) | 264 | 65,158 | `packages/client/ui-*` | Web GUI components |
| Python | 21 | 4,311 | `python/` SDK + runtime carrier, `scripts/*.py`, `code-runtime-python/py/protocol.py` | consumer SDK driving the harness over ndjson JSON-RPC stdio; build helpers |
| Rust | 1 | 363 | `native/landlock-run/packages/entry/native/src/main.rs` | static Linux launcher binary enforcing Landlock LSM rules (prebuilt for linux-x64/arm64; wrapped by a TS entry package) |
| SQL DDL | 56 | 181 | storage/session-query schemas | SQLite persistence (monotonic `SCHEMA_VERSION`) |
| CSS | 113 | 15,236 | web UI | styling |
| Shell/JS glue | ~50 | small | `scripts/*.sh`, `.mjs`, preset-loader fixtures | release/build plumbing; JS fixtures exist because preset plugins are loaded as JavaScript |

## 5. Why each language choice is right — change/no-change analysis

### 5.1 Agent tools: TypeScript — KEEP

1. **The architecture is a typed same-process plugin graph.** Capability seams are Service Definition / Provider / Consumer roles resolved as compile-time TypeScript interfaces (vendored Cordis DI). Splitting any tool into another language forces a wire protocol, serialization, and duplicate validation at that seam — deleting exactly the guarantees the repo standardizes on (branded opaque ids, closed unions ending in `assertNever`, required-on-read `SessionEventMap`, "trust TypeScript at typed same-process boundaries").
2. **The workload is I/O-bound.** Every expensive step already happens outside the interpreter: model completions (seconds over HTTPS), shell commands (executed by `pwsh.exe`/`bash` themselves), web search (external APIs), persistence (SQLite), subagents (child processes), workflow orchestration (worker threads). There is no profiled CPU hotspot in any tool implementation that a systems language would improve; interpreter overhead is noise against model latency.
3. **Concurrency needs are already met** without a host-language change: `node:worker_threads` for workflows, child processes/subprocess providers for isolation, async I/O everywhere else.
4. **The quality infrastructure is TypeScript-native and strict**: per-file 100% coverage gate on `src`, cross-file duplication gate, knip/publint hygiene, `typecheck`, snapshot replays, `doc-sync`. Porting any tool resets its entire test/gate story to zero while adding a second build toolchain.
5. **Ecosystem fit**: Bun/Node give first-class `fs`, `child_process`, workers, and HTTP; the pre-release stance permits free renames/repackaging *within* TypeScript, so there is no architectural pressure pointing outward.

Cost/benefit of a hypothetical port (per tool or wholesale): new IPC boundary + schema duplication + loss of branded types across it + duplicated tests and CI — against no measurable performance or correctness gain. **Do not port.**

### 5.2 Rust: the Landlock launcher — KEEP (correctly not TypeScript)

`main.rs` implements a launcher that applies Linux Landlock rules before exec. That requires raw syscalls and produces small static prebuilt binaries; neither is natural in Node (it would mean an addon toolchain dragged into every install). Keeping it a standalone binary keeps Windows/macOS builds free of a Rust toolchain, and the TS entry package wraps it cleanly — the unsafe/systems edge lives in Rust, orchestration stays TS. At 363 lines the maintenance surface is minimal, and CI owns the release matrix. Revisit only if the sandbox mechanism itself is replaced.

### 5.3 Python: SDK + bundled runtime carrier — KEEP (audience-driven)

`python/` exists so Python developers can drive the harness as a subprocess over newline-delimited JSON-RPC on stdio. Its entire value is being Python; it contains no agent-side tool logic and duplicates nothing that would benefit from consolidation. The small `py/protocol.py` helper under `code-runtime-python` likewise speaks Python *to* Python runtimes by design.

### 5.4 PowerShell/Bash: product surface, not implementation choice

The shell tools' job is to run the platform's shell. Their "language" is what the user asked to run; the tool wrapper, sandboxing (`pwsh-sandbox`, `bash-sandbox`, Windows ACL sandbox), and result presentation are all TypeScript. Changing this would change the product, not improve the implementation.

### 5.5 SQL, CSS, website, build scripts — appropriate as-is

SQLite DDL is declarative data definition with a versioned migration policy; CSS styles the GUI; the VitePress site is standard; the handful of `.py`/`.mjs`/`.sh` release scripts do one job each.

## 6. Triggers that would justify revisiting (none currently met)

1. A **profiled, CPU-bound hot path inside a tool's own code** (e.g., heavy diffing or media encoding). Even then, the right move is a native accelerator behind a pure function, not moving the tool out of TypeScript.
2. A **required dependency that only exists in another ecosystem** — wrap it as an external process/provider (the pattern already used for pwsh, CDP browser, LSP servers), don't port the harness.
3. An **embedding constraint** (Python-only host) — use the existing Python SDK; it exists precisely for that.
4. Replacement of the **Linux sandbox mechanism** — only then does the single Rust file's reason to exist change.

## 7. Watch items

- `vendor/` holds pinned Cordis source copies — sync discipline (manifest + upstream SHAs) is the standing cost of the vendoring choice, not a language problem.
- The preset-loader `.js` fixtures and generated `.mjs` test artifacts are intentional: plugins load as JavaScript at runtime even though sources are TypeScript.
- `str_replace_editor` beside the native `read`/`write`/`edit` suite is a deliberate two-vocabulary design, not drift: the implemented single-editor decision (`.agents/notes/implemented/simplification/2026-08-10-default-presets-single-editor.md`) keeps general-purpose presets on the native suite only, retains `str_replace_editor` as the `minimal` preset's dedicated editor (and Python runtime default), and leaves explicit mounts available. The shared `bundle/base` row is a neutral default that later patch layers disable per surface.

## Appendix — evidence

Commands executed for this audit (this session): repo/package enumeration (`Get-ChildItem`, glob over `packages/*/*/package.json` → 231 workspace packages); extension census via `git ls-files` (7,965 tracked files; counts in §4); line counts per language over tracked files; registration greps locating every tool id (`name: '...'`) in package `src` trees, e.g. `goal/tool-goal/src/index.ts:196-235`, `shell/tool-pwsh/src/index.ts:253`, `interaction/tool-ask-user/src/index.ts:21`, `todo/tool-todo/src/index.ts:150`, `skill/tool-skill/src/index.ts:82`, `jobs/tool-jobs/src/index.ts:303-363`, `subagent/tool-subagent-control/src/{index,list-agents}.ts`, `fs/tool-fs/src/{read,write,edit,read-image}.ts`, `fs/tool-fs-search/src/{glob,grep}.ts`, `plan/plan-mode/src/index.ts:280,68`; `native/landlock-run` tree inspection (Cargo.toml, `src/main.rs`, prebuilds); `workflow-worker-thread` `node:worker_threads` imports confirmed.
