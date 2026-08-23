# OpenMem — Agent Memory System for AI Coding Agents

Persistent semantic memory for AI coding agents. OpenMem indexes your agents'
real session history into a local LanceDB vector store, retrieves relevant
memories on demand, reflects on sessions to extract facts and lessons, and —
since v2.x — exposes the whole memory layer as a native **MCP server** so any
MCP-capable client can `remember` / `recall` without file-based integration.

Works with **Claude Code** and **Codex CLI** session formats today (evidence-based
parsers), tolerates absent history gracefully elsewhere, and includes a generic
file-based fallback.

> **Status:** actively developed. The storage layer, parsers, learning cycle,
> MCP server, and evaluation harness are implemented and covered by a 225-test
> suite. LLM-backed reflection activates automatically when an API key is
> present; without one everything runs in documented heuristic mode.
> Retrieval quality is measured, not claimed — see [Evaluation](#evaluation).

## What It Does (verified)

- **Real session parsing** — typed-record JSONL parsers built from on-disk
  evidence of Claude Code (`~/.claude/projects/**/*.jsonl`) and Codex CLI
  (`~/.codex/sessions/**/rollout-*.jsonl`), with noise filtering (auth-error
  spam, CLI echoes, sidechain transcripts) and malformed-line tolerance.
  Formats are documented in [`doc/session_formats.md`](doc/session_formats.md).
- **Semantic memory store** — LanceDB with fixed-size vector columns,
  deterministic IDs, float64 importance scores, automatic schema migration,
  BGE cross-encoder reranking when the `ml` extra is installed, and an honest
  keyword-fallback search when it is not.
- **Tiered memory** — daily → weekly → long-term consolidation with stable,
  process-independent content hashing; re-runs are idempotent.
- **Reflection loop** — per-session analysis producing facts, improvements,
  and memories. Mode-tagged `llm` or `heuristic`; malformed LLM output falls
  back visibly instead of silently.
- **Outcome-grounded improvements** — improvement queue items can only be
  completed with linked evidence (memory id / session id / explicit user
  confirmation). No self-completion theater.
- **MCP server** — `remember`, `recall`, `context`, `profile`, `stats`,
  `forget` tools over stdio; see [`doc/mcp_integration.md`](doc/mcp_integration.md).
- **Evaluation harness** — golden retrieval benchmark with recall@k / MRR /
  nDCG@k / fallout metrics and a regression gate wired into the test suite.
- **11 agent integrations** — skill/context-file installation for Claude Code,
  Codex CLI, Cursor, VS Code, Windsurf, Qwen Code, OpenCode, Antigravity IDE,
  Kilo CLI, OpenClaw, plus a generic adapter — all generated from a single
  template source (`bin/generate_skills.py`) so they cannot drift apart.

## Quick Start

```bash
git clone https://github.com/ncdevshiv/openmem.git
cd openmem

# create an environment and install (core deps only)
python -m venv .venv && .venv\Scripts\activate     # Windows
pip install -e .

# initialize the store and check health
python main.py status

# index your real agent history and run one learning cycle
python main.py run-cycle

# search your own memory
python main.py search "deepseek harness"

# measure retrieval quality (writes data/eval/latest.json)
python main.py eval
```

Optional extras:

```bash
pip install -e ".[ml]"    # torch + sentence-transformers + transformers (embeddings & reranker)
pip install -e ".[mcp]"   # MCP server support (mcp>=2.0.0)
pip install -e ".[llm]"   # litellm for LLM-backed reflection
```

Copy `config.example.json` to `config.json` if you want to override defaults;
the repo never ships your machine-specific config.

### Enabling AI features

LLM reflection activates automatically when a provider is reachable:

| Provider | Environment variable | Default model |
|---|---|---|
| OpenAI | `OPENAI_API_KEY` | `gpt-4o-mini` |
| Anthropic | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` |
| Gemini | `GEMINI_API_KEY` | `gemini-pro` |
| Ollama | `OLLAMA_BASE_URL` | `llama3` |

Without keys, zero network calls are made and reflections are tagged
`"mode": "heuristic"`. With keys, reflections are tagged `"mode": "llm"` and
cycle reports include a `reflection_modes` summary. Malformed LLM output is
rejected and falls back with a visible warning.

## Commands

```
python main.py status              # system health
python main.py run-cycle           # full learning cycle (idempotent)
python main.py run-cycle --full    # re-index from scratch window
python main.py search <query>      # semantic/keyword memory search
python main.py eval [--report P]   # retrieval benchmark (markdown + JSON)
python main.py profile             # learned user profile
python main.py stats               # store statistics
python main.py --agents            # list supported adapters
python main.py --skill <agent>     # install skill files for an agent
```

## Evaluation

Retrieval quality is measured against a versioned golden set (16 queries over
a deterministic 36-memory corpus: exact-term, paraphrase, and negative classes)
with a regression gate wired into the test suite. Baseline (keyword-fallback
mode, no reranker):

| class      | queries | recall@5 | MRR   | nDCG@5 | fallout@5 |
|------------|--------:|---------:|------:|-------:|----------:|
| exact_term | 6       | 0.972    | 1.000 | 1.000  | 0.333     |
| paraphrase | 6       | 1.000    | 0.889 | 0.917  | 0.300     |
| negative   | 4       | 0.000    | 0.000 | 0.000  | 0.000     |
| aggregate  | 16      | 0.740    | 0.708 | 0.719  | 0.237     |

Thresholds and rationale live in [`eval/BASELINE.md`](eval/BASELINE.md); the
gate fails CI if retrieval regresses below them. Known weaknesses of the
current keyword matcher (substring false positives like *port* ⊂ *report* /
*passport*, frequency-rewarding tie-breaks) are logged there as the exact
targets for the future reranker/embedder work.

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│ Your agent          Any MCP-capable client                 │
│ Claude Code etc.    (remember/recall/context/profile/...)  │
└───────┬───────────────────────────┬────────────────────────┘
        │ skill files +             │ stdio (mcp>=2.0)
        │ context injection         │
┌───────▼───────────┐      ┌────────▼─────────┐
│ agents/*          │      │ mcp_server.py    │
│ real session      │      └────────┬─────────┘
│ parsers + skills  │               │
└───────┬───────────┘               ▼
        ▼                    memory_store (same core)
┌────────────────────────────────────────────────────────────┐
│ learning_loop/   scheduler · indexer · patterns · reflect  │
├────────────────────────────────────────────────────────────┤
│ memory_store/    vector_db (LanceDB) · tiers · user model  │
│                  skill generator · retrieval metrics       │
├────────────────────────────────────────────────────────────┤
│ core/llm.py      litellm wrapper — lazy, network-free init │
├────────────────────────────────────────────────────────────┤
│ eval/            golden corpus · queries · runner · gate   │
└────────────────────────────────────────────────────────────┘
```

Directory map:

```
main.py              entry point (delegates to bin/launcher.py)
mcp_server.py        MCP stdio server
openmem_cli.py       console-script wrapper (pip install -e .)
agents/              adapter contract (base.py) + per-agent parsers/skills
memory_store/        vector DB, tier manager, user model, skill gen, metrics
learning_loop/       scheduler, conversation indexer, pattern recognizer,
                     reflection engine
autonomous/          EXPERIMENTAL optimizer/evolution scaffolds (not yet
                     wired into the cycle — see roadmap)
core/                provider-agnostic LLM abstraction
eval/                golden corpus, queries, runner, baseline
bin/                 launcher, installer, config/skill generators
doc/                 session format inventory, MCP integration guide
tests/               225-test suite (unit + integration + gates)
```

## Privacy

Everything is local-first. Indexed content lives in `data/lancedb/`
(gitignored), the MCP server talks over stdio, and no network call happens
unless you explicitly configure an LLM provider. Tests run hermetically in
temp directories and provably never touch the live store.

## Testing

```bash
python -m unittest discover -s tests     # 225 tests
```

Includes unit tests for every module, parser fixtures mirroring real on-disk
formats, MCP end-to-end subprocess tests, LLM boundary mocks, a retrieval
regression gate, and leakage checks proving the suite leaves the live store
byte-identical.

## Roadmap

- Reranker/embedder integration for semantic retrieval (levers already
  identified by the eval: morphology-aware matching, IDF weighting, semantic
  tie-breaking)
- Wire `autonomous/` evolution scaffolds to real fitness signals from the
  eval harness
- Sleep-time consolidation ("dream cycles") measured by eval lift
- Cross-agent shared memory namespaces via MCP

## License

[MIT](LICENSE) — © Shivam Tiwari
