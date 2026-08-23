"""
Hermetic golden corpus for OpenMem retrieval evaluation.

Builds a fully deterministic fixture memory store in a TEMP LanceDB via the
standard VectorDB constructor -- never the live data/lancedb store. Every
fixture carries an explicit id ("golden-*") so relevance judgments in
eval/golden_queries.json can be expressed as exact-id sets and stay valid
across runs and machines.

Design constraints:

- Deterministic: same inputs -> byte-identical corpus contents and ids.
  Ranking in keyword-fallback mode is stable (per-term hit counts with a
  stable sort over insertion order), so measured metrics reproduce exactly
  on an unchanged corpus + query set.
- >= 30 rows across >= 3 topics: two real-history-derived topics
  ("deepseek harness port", "nc-code repo"), one internals topic
  (retrieval machinery), and three unrelated distractor topics that make
  false positives possible and measurable.
- Negative-query safety: distractor topics deliberately avoid every term
  used by negative golden queries; run_eval validates this at runtime and
  fails loudly if future edits break it.

Two deliberate lexical traps are kept INSIDE the corpus ("passport" and
"Airport" contain the substring "port") so the eval exposes how substring
matching behaves on morphological collisions instead of hiding it.
"""

import sys
from pathlib import Path
from typing import Dict, List

BASE_DIR = Path(__file__).parent.parent
if str(BASE_DIR) not in sys.path:
    sys.path.insert(0, str(BASE_DIR))

from memory_store.vector_db import VectorDB

GOLDEN_CORPUS_VERSION = "1.0.0"

# Session ids group fixtures by topic; metadata marks rows as fixtures so a
# stray golden row in a live store is self-identifying.
_SESSION_BY_TOPIC = {
    "dsh": "golden-session-dsh",
    "ncc": "golden-session-ncc",
    "ret": "golden-session-ret",
    "grd": "golden-session-grd",
    "bak": "golden-session-bak",
    "trv": "golden-session-trv",
}

_TOPIC_LABELS = {
    "dsh": "deepseek harness port",
    "ncc": "nc-code repo",
    "ret": "retrieval internals",
    "grd": "gardening (distractor)",
    "bak": "baking (distractor)",
    "trv": "travel packing (distractor)",
}

# (id_prefix, [(suffix_number, content, importance, tags), ...])
# Content strings are the ground truth; ids are stable across runs.
_TOPIC_FIXTURES = {
    "dsh": [
        (1, "Ported the DeepSeek harness web GUI to a local dev server and "
            "verified boot payload injection on port 3080.", 0.9, ["harness", "porting"]),
        (2, "The DeepSeek harness checkout lives under an npx cache folder; "
            "the install location stays separate from the working directory.",
         0.8, ["harness", "layout"]),
        (3, "Client-plugin HMR reloads bundles only while the dev watcher "
            "rebuilds them inside the DeepSeek harness.", 0.7, ["harness", "tooling"]),
        (4, "The orchestrator delegates hands-on execution to subagents and "
            "supervises their full lifecycle in the DeepSeek harness.",
         0.9, ["harness", "orchestration"]),
        (5, "Long commands stream tool output through managed background "
            "jobs that report job ids in the DeepSeek harness.",
         0.6, ["harness", "jobs"]),
        (6, "Approval prompts are disabled for this harness session; sandbox "
            "escalation requests fail closed in the DeepSeek harness.",
         0.7, ["harness", "permissions"]),
    ],
    "ncc": [
        (1, "Cloning the nc-code repository needs git config "
            "safe.directory=F:/nc-code before git trusts the workspace.",
         0.8, ["nc-code", "git"]),
        (2, "Committed the parser fixes to the nc-code repo after running "
            "git commit with -c safe.directory set.", 0.9, ["nc-code", "git"]),
        (3, "Release checklist for nc-code: bump the version, tag the "
            "commit, publish the changelog.", 0.6, ["nc-code", "release"]),
        (4, "Session transcripts live under .nc-code so replays and diffs "
            "stay reproducible.", 0.7, ["nc-code", "history"]),
        (5, "Branch protection on nc-code blocks direct pushes; changes "
            "land only through pull request merges.", 0.8, ["nc-code", "workflow"]),
        (6, "Refactored the nc-code indexer to stream files lazily instead "
            "of loading the whole codebase.", 0.7, ["nc-code", "refactoring"]),
    ],
    "ret": [
        (1, "OpenMem keeps memories in LanceDB with a fixed-size vector "
            "schema; variable-size columns break ANN search on lancedb 0.37.",
         0.9, ["storage", "schema"]),
        (2, "Keyword fallback search scores each row by the fraction of "
            "distinct query terms matched, tie-broken by total term "
            "frequency.", 0.9, ["search", "ranking"]),
        (3, "Scores use float64 end to end so importance values round-trip "
            "exactly without float32 drift.", 0.8, ["search", "precision"]),
        (4, "Hermetic tests build temporary stores and never write the live "
            "data directory; a leakage gate enforces it.", 0.8, ["testing"]),
        (5, "IVF_PQ index training needs at least 256 rows; smaller tables "
            "fall back to a brute-force scan.", 0.7, ["storage", "index"]),
        (6, "The MCP server exposes six tools, including remember, recall, "
            "and forget, over the memory store.", 0.7, ["integration"]),
    ],
    "grd": [
        (1, "Tomato beds get compost tea every Friday morning until the "
            "first frost.", 0.5, ["garden"]),
        (2, "The worm bin turns kitchen scraps into dark castings in about "
            "eight weeks.", 0.5, ["garden", "compost"]),
        (3, "Prune the roses before winter to keep cane borers away.",
         0.4, ["garden"]),
        (4, "Mulch around fruit trees should stay shallower than four "
            "inches.", 0.4, ["garden"]),
        (5, "Drip irrigation lines get flushed at the start of each growing "
            "season.", 0.4, ["garden"]),
        (6, "Heirloom seedlings hardened off over a week transplant without "
            "shock.", 0.4, ["garden"]),
    ],
    "bak": [
        (1, "Feed the sourdough starter with equal weights of flour and "
            "water every twelve hours.", 0.5, ["baking"]),
        (2, "Bulk fermentation ends once the dough has risen about thirty "
            "percent.", 0.5, ["baking"]),
        (3, "A preheated dutch oven gives the loaf its strongest oven "
            "spring.", 0.4, ["baking"]),
        (4, "Gentle shaping protects the gas cells and improves crumb "
            "structure.", 0.4, ["baking"]),
        (5, "Scoring the boule at forty-five degrees produces a pronounced "
            "ear.", 0.4, ["baking"]),
        (6, "Cold retard overnight deepens flavor and makes the dough "
            "easier to handle.", 0.4, ["baking"]),
    ],
    "trv": [
        (1, "Pack the passport in the carry-on bag, never in checked "
            "luggage.", 0.6, ["travel"]),
        (2, "Rolling clothes saves more space than folding them flat.",
         0.4, ["travel"]),
        (3, "A universal power adapter covers most outlets overseas.",
         0.4, ["travel"]),
        (4, "Hiking boots need a week of breaking in before any long trek.",
         0.4, ["travel"]),
        (5, "Travel insurance paperwork belongs in the waterproof pouch.",
         0.5, ["travel"]),
        (6, "Airport security lines move faster before dawn departures.",
         0.4, ["travel"]),
    ],
}


def golden_corpus_specs() -> List[Dict]:
    """
    Return the full fixture list as dicts, in canonical insertion order.

    Returns:
        List of memory dicts with keys: id, content, session_id,
        importance, tags, metadata
    """
    specs: List[Dict] = []
    for prefix in ("dsh", "ncc", "ret", "grd", "bak", "trv"):
        for number, content, importance, tags in _TOPIC_FIXTURES[prefix]:
            mid = f"golden-{prefix}-{number:02d}"
            specs.append({
                "id": mid,
                "content": content,
                "session_id": _SESSION_BY_TOPIC[prefix],
                "importance": float(importance),
                "tags": list(tags),
                "metadata": {
                    "fixture": True,
                    "corpus_version": GOLDEN_CORPUS_VERSION,
                    "topic": _TOPIC_LABELS[prefix],
                    "topic_key": prefix,
                },
            })
    return specs


def build_golden_corpus(db_path: str) -> VectorDB:
    """
    Build the golden corpus in a dedicated (temp) LanceDB store.

    Uses the standard VectorDB constructor with an explicit db_path --
    identical to production storage, pointed at disposable disk. Rows are
    inserted with their deterministic explicit ids via
    add_memory(memory_id=...) so golden_queries.json judgments hold forever.

    Args:
        db_path: Directory for the temporary LanceDB store. Callers own
            this path's lifecycle (tests: tempfile.mkdtemp(); run_eval:
            auto-created and auto-cleaned temp dir).

    Returns:
        The populated VectorDB instance (caller closes/cleans up)

    Raises:
        RuntimeError: If any fixture failed to insert (row count mismatch),
            which would silently invalidate downstream metrics.
    """
    db = VectorDB(db_path=db_path)
    specs = golden_corpus_specs()
    inserted_ids = []
    for spec in specs:
        mid = db.add_memory(
            content=spec["content"],
            session_id=spec["session_id"],
            importance=spec["importance"],
            tags=spec["tags"],
            metadata=spec["metadata"],
            auto_embed=False,  # offline-safe; vectors are NULL in keyword mode
            memory_id=spec["id"],
        )
        if mid != spec["id"]:
            raise RuntimeError(
                f"[Eval] Golden fixture insert failed: expected id "
                f"{spec['id']!r}, got {mid!r}"
            )
        inserted_ids.append(mid)

    stored = len(db)
    if stored != len(specs):
        raise RuntimeError(
            f"[Eval] Golden corpus incomplete: {stored} rows stored, "
            f"{len(specs)} expected"
        )
    return db
