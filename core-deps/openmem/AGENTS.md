# AGENTS.md - OpenMem Development Guide

Agent memory system: real session parsing, LanceDB-backed semantic search, tiered consolidation, reflection (LLM or heuristic), an MCP server, and a measured retrieval benchmark.

## Project Structure

```
openmem/
├── main.py                   # Entry point (delegates to bin/launcher.py)
├── mcp_server.py             # MCP stdio server (remember/recall/context/...)
├── openmem_cli.py            # Console-script wrapper
├── memory_store/             # Core memory systems
│   ├── vector_db.py          # LanceDB vector store (fixed-size vectors,
│   │                         #   schema migration, keyword fallback search)
│   ├── memory_manager.py     # Memory tier management (stable sha256 ids)
│   ├── user_model.py         # User profiling
│   ├── skill_generator.py    # Skill auto-generation
│   └── retrieval_metrics.py  # recall@k / MRR / nDCG / fallout (pure)
├── learning_loop/            # Learning engine
│   ├── conversation_indexer.py  # Adapter-driven indexing + content dedup
│   ├── pattern_recognizer.py
│   ├── reflection_engine.py  # Mode-tagged (llm/heuristic), evidence-gated
│   └── scheduler.py          # 5-phase cycle with per-phase error reporting
├── autonomous/               # EXPERIMENTAL scaffolds (not wired into cycle)
│   ├── self_optimizer.py
│   └── self_evolution.py
├── core/llm.py               # litellm wrapper — lazy, network-free init
├── agents/                   # Agent contract (base.py) + 11 adapters with
│                             #   real session parsers (claude_code, codex_cli)
├── eval/                     # Golden corpus, queries, runner, BASELINE.md
├── skills/                   # OpenClaw integration + legacy archive
├── bin/                      # launcher, installer, generators
├── doc/                      # session_formats.md, mcp_integration.md
└── tests/                    # 225-test unittest suite (incl. gates)
```

## Build/Lint/Test Commands

```bash
# Run the full suite (must stay green; includes retrieval gate)
python -m unittest discover -s tests

# Run one file / class / test
python -m unittest tests.test_memory_store
python -m unittest tests.test_memory_store.TestVectorDB
python -m unittest tests.test_memory_store.TestVectorDB.test_add_memory

# Application commands
python main.py status        # System status
python main.py run-cycle     # Full learning cycle (idempotent)
python main.py run-cycle --full
python main.py search "query" --limit 10
python main.py eval          # Retrieval benchmark -> markdown + data/eval/latest.json
python main.py profile       # Show user profile
python main.py stats         # Show statistics

# Install (editable, core only) + extras
pip install -e .
pip install -e ".[ml]"   # embeddings/reranker (torch, sentence-transformers)
pip install -e ".[mcp]"  # MCP server SDK
pip install -e ".[llm]"  # litellm provider support
```

## Development Rules

- **Never let tests touch the live `data/lancedb`** — inject temp paths via the
  existing DI patterns (`VectorDB(db_path=...)`, engine kwargs). The suite is
  leakage-checked.
- **Retrieval gate** — `tests/test_retrieval_gate.py` fails below thresholds in
  `eval/BASELINE.md`. If you change search behavior, re-run
  `python main.py eval`, update BASELINE.md deliberately, and note why numbers moved.
- **Improvements are evidence-gated** — `complete_improvement()` requires
  `evidence_memory_id` / `evidence_session_id` / `confirmed_by="user"`.
- **Config** — never commit `config.json` (machine-specific); use
  `config.example.json` as the template. `OPENMEM_DB_PATH` overrides db_path.
- **Heavy deps stay optional** — torch/sentence-transformers/transformers live
  in the `ml` extra; code must degrade gracefully and honestly (no fake data).


## Code Style Guidelines

### General
- Python 3.x, type hints from `typing` module (`List`, `Dict`, `Optional`, `Any`)
- f-strings for formatting, 4 spaces indentation, lines under 120 chars

### Imports
Standard library → third-party → local. Use `sys.path.insert(0, str(BASE_DIR))` at module top. Group by type:

```python
import os, sys, json
from pathlib import Path
from typing import List, Dict, Any, Optional

try:
    import lancedb
    LANCEDB_AVAILABLE = True
except ImportError:
    LANCEDB_AVAILABLE = False

from memory_store import get_vector_db
```

### Naming
- Classes: `PascalCase` (e.g., `MemoryManager`)
- Functions/Variables: `snake_case` (e.g., `get_memory_context`)
- Constants: `UPPER_SNAKE_CASE` (e.g., `USER_PROFILE_SCHEMA`)
- Private methods: prefix `_` (e.g., `_init_db`)

### Data Classes
```python
from dataclasses import dataclass

@dataclass
class Memory:
    id: str
    content: str
    session_id: Optional[str]
    timestamp: str
    importance: float
    tags: List[str]
    metadata: Dict[str, Any]
    vector: Optional[List[float]] = None
```

### Docstrings
Google-style for public functions/classes:
```python
def search_memory(self, query: str, n_results: int = 5) -> List[Dict]:
    """
    Search across all memory tiers.

    Args:
        query: Search query text
        n_results: Maximum number of results

    Returns:
        List of matching memory dicts with scores
    """
```

### Error Handling
- Specific exception types when possible
- Return `None`/empty collections for "not found" cases
- Try/except for optional dependencies:
```python
try:
    import lancedb
    LANCEDB_AVAILABLE = True
except ImportError:
    LANCEDB_AVAILABLE = False
    print("[LanceDB] Not installed. Run: pip install lancedb")
```

### Database Patterns (Singleton)
```python
_db_instance = None

def get_vector_db() -> LanceDBVectorStore:
    global _db_instance
    if _db_instance is None:
        _db_instance = LanceDBVectorStore()
    return _db_instance
```
- Close connections in `finally` or `close()` methods
- Use `tempfile.mkdtemp()` for test fixtures, clean in `tearDown`

### Testing Patterns
```python
class TestVectorDB(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.db = VectorDB(db_path=os.path.join(self.test_dir, "test_vectordb"))

    def tearDown(self):
        self.db.close()
        shutil.rmtree(self.test_dir, ignore_errors=True)
```
- Descriptive names: `test_add_memory`, `test_search_session_filter`
- Use `assertIsNotNone`, `assertIsInstance`, `assertEqual`

### Logging
Print statements with prefixes: `[LanceDB]`, `[OpenMem]`. Errors: `traceback.print_exc()`.

### Path Handling
```python
BASE_DIR = Path(__file__).parent
sys.path.insert(0, str(BASE_DIR))
```
Use `os.path.join()` for cross-platform paths.

### CLI Structure (main.py pattern)
```python
def cmd_run_cycle(args):
    scheduler = LearningScheduler()
    report = scheduler.run_cycle(full=args.full)
    return 0 if report.get('success') else 1
```
Use argparse with subparsers, return 0 for success, 1 for failure.

## Key Dependencies

Core: `lancedb`, `pyarrow`, `numpy`, `python-dateutil`, `tqdm`.
Extras: `ml` (torch, sentence-transformers, transformers, pandas), `mcp`
(mcp>=2.0.0), `llm` (litellm), `honcho`. Heavy deps must stay optional —
every module degrades gracefully without them.

## Architecture

### Memory Tiers
1. **Daily** (importance 0.6): Raw events and logs
2. **Weekly** (0.7): Condensed from daily
3. **Long-term** (0.9): Important distilled facts

### Vector Store
- LanceDB-backed for sub-millisecond search
- Auto-embedding via `all-MiniLM-L6-v2`
- BGE Reranker for improved relevance scoring
  - GPU: `BAAI/bge-reranker-large` (best quality)
  - CPU: `BAAI/bge-reranker-base` (good quality, fast)
- Schema evolution supported

### Pattern Recognition
- Request types: `factual_question`, `build_request`, `problem_solving`, etc.
- Response strategies: `concise_direct`, `structured_format`, `code_oriented`

## File Locations
- Database: `data/lancedb/` (override with `OPENMEM_DB_PATH`)
- Memory metadata: `data/memory/memory_meta.db` (SQLite)
- Cycle/index state: `data/scheduler_state.json`, `data/sessions/index_state.json`
- Eval reports: `data/eval/latest.json`; thresholds in `eval/BASELINE.md`
- Generated skills: `generated_skills/` (gitignored)
- OpenClaw workspace: `~/.openclaw/workspace`
