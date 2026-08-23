# OpenMem Documentation

**OpenMem** is a local-first memory layer for AI coding agents: it parses your
agents' real session history, stores it in LanceDB, reflects on sessions to
extract facts and lessons, and serves memory natively over MCP.

Start with the main [README](../README.md). This directory holds deeper guides.

## Guides

| Document | Contents |
|---|---|
| [`session_formats.md`](session_formats.md) | Evidence-based inventory of the agent history formats OpenMem parses: Claude Code JSONL record types, Codex CLI rollouts, Cursor (empty-tolerant), noise filtering rules |
| [`mcp_integration.md`](mcp_integration.md) | Wiring the MCP server into Claude Code, Cursor, or any stdio MCP client; tool reference; isolation via `OPENMEM_DB_PATH`; troubleshooting |
| [`../eval/BASELINE.md`](../eval/BASELINE.md) | Measured retrieval baseline (recall@k / MRR / nDCG / fallout), regression-gate thresholds and rationale |

## Quick Reference

```bash
python main.py status              # health check
python main.py run-cycle           # index sessions + reflect + consolidate (idempotent)
python main.py search "<query>"    # search memory
python main.py eval                # retrieval benchmark
python -m mcp_server               # run the MCP server (stdio)
python -m unittest discover -s tests   # 225-test suite
```

## Historical Note

Earlier revisions of this project were branded **LanceMem** and documented a
`/learn` command interface that no longer exists. Current triggers are
`/mem` (most agents), `@memory` (Cursor/Windsurf), `/lm` (OpenClaw), or —
preferred — the MCP tools. References to `from lance_mem import LanceMem`
are obsolete; the programmatic entry points are
`memory_store.vector_db.get_vector_db()` and `core.llm.get_llm()`.
