"""
Retrieval-quality eval runner for OpenMem.

Executes the versioned golden queries (eval/golden_queries.json) against
the hermetic golden corpus (eval/golden_corpus.py, built in a TEMP
LanceDB), scores every query with memory_store.retrieval_metrics, and
writes a JSON report plus a rendered markdown table.

Honesty contract:

- The retrieval mode actually used is auto-detected from the store and
  reported verbatim: "keyword-fallback" when no embedder is loaded
  (today's network-free environment), "vector" only when a real embedding
  model is present. The runner never pretends semantic numbers came out of
  hash embedders or offline stubs -- with no embedder, search() itself
  routes to keyword fallback.
- Empty/absent results produce honest zero metrics, never exceptions.

Usage:
    python -m eval.run_eval [--report PATH] [--keep-db]
    # or importable: from eval.run_eval import run_eval, render_markdown_report

The CLI defaults --report to <repo>/data/eval/latest.json. Library callers
pass report_path=None to skip writing entirely (used by the regression-gate
test, which keeps its temp artifacts under its own tmpdir).
"""

import argparse
import json
import shutil
import sys
import tempfile
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional, Tuple

BASE_DIR = Path(__file__).parent.parent
if str(BASE_DIR) not in sys.path:
    sys.path.insert(0, str(BASE_DIR))

from eval.golden_corpus import GOLDEN_CORPUS_VERSION, build_golden_corpus, golden_corpus_specs
from memory_store.retrieval_metrics import evaluate_ranked_query
from memory_store.vector_db import VectorDB

GOLDEN_QUERIES_PATH = Path(__file__).parent / "golden_queries.json"
DEFAULT_REPORT_PATH = BASE_DIR / "data" / "eval" / "latest.json"
EVAL_K = 5


def detect_retrieval_mode(db: VectorDB) -> Tuple[str, Dict]:
    """
    Detect which search path VectorDB.search() will actually take.

    Args:
        db: A constructed VectorDB instance

    Returns:
        (mode, capabilities) where mode is "vector" only when a real
        sentence-transformers embedder is loaded (search() routes there),
        "keyword-fallback" otherwise; capabilities reports reranker/GPU
        availability honestly for the report header.
    """
    stats = db.get_stats()
    embedder_loaded = bool(stats.get("embedder_available"))
    mode = "vector" if embedder_loaded else "keyword-fallback"
    reranker = stats.get("reranker", {})
    capabilities = {
        "embedder_available": embedder_loaded,
        "reranker_available": bool(reranker.get("available")),
        "reranker_loaded": bool(reranker.get("loaded")),
        "gpu_enabled": bool(reranker.get("gpu_enabled")),
    }
    return mode, capabilities


def load_golden_queries(path: Optional[Path] = None) -> Dict:
    """
    Load and minimally validate the golden queries file.

    Args:
        path: Overrides the default versioned file (tests may pass their own)

    Returns:
        Parsed golden queries dict

    Raises:
        ValueError: If required structure (version, classes, per-query ids)
            is missing -- a corrupt golden file must fail loudly, not score 0.
    """
    path = path or GOLDEN_QUERIES_PATH
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)

    if not isinstance(data, dict):
        raise ValueError(f"[Eval] Golden queries root must be an object: {path}")
    if not data.get("version"):
        raise ValueError("[Eval] Golden queries missing 'version'")
    classes = data.get("classes") or {}
    total = 0
    for class_name, block in classes.items():
        queries = (block or {}).get("queries") or []
        for q in queries:
            if not q.get("id") or not q.get("query"):
                raise ValueError(
                    f"[Eval] Query in class '{class_name}' missing id/query"
                )
            if "relevant_ids" not in q:
                raise ValueError(
                    f"[Eval] Query '{q['id']}' missing explicit relevant_ids "
                    f"(empty list is valid; omission is not)"
                )
        total += len(queries)
    if total == 0:
        raise ValueError("[Eval] Golden queries contain no queries")
    return data


def iter_class_queries(golden: Dict) -> List[Tuple[str, Dict]]:
    """Flatten classes -> ordered [(class_name, query_dict), ...] list."""
    flat: List[Tuple[str, Dict]] = []
    for class_name, block in golden.get("classes", {}).items():
        for q in (block or {}).get("queries", []):
            flat.append((class_name, q))
    return flat


def _validate_negative_disjointness(corpus_contents: List[str], negative_queries: List[Dict]) -> None:
    """
    Fail loudly if any negative query term lexically overlaps any corpus doc.

    Keyword-fallback matches case-insensitive substrings, so even partial
    word collisions (e.g. 'patterns' inside 'design patterns') would make a
    negative query dishonest. Golden integrity beats silent metric drift.
    """
    lowered_docs = [c.lower() for c in corpus_contents]
    for q in negative_queries:
        terms = [t.strip().lower() for t in q["query"].split() if t.strip()]
        for term in terms:
            hits = [i for i, c in enumerate(lowered_docs) if term in c]
            if hits:
                raise ValueError(
                    f"[Eval] Negative query '{q['id']}' term {term!r} appears "
                    f"in corpus docs {[i for i in hits]}; negative judgment "
                    f"is invalid until corpus or query is fixed"
                )


def _mean(values: List[float]) -> float:
    """Arithmetic mean of non-empty list (0.0 for empty)."""
    if not values:
        return 0.0
    return sum(values) / len(values)


def run_eval(
    report_path: Optional[Path] = None,
    db_path: Optional[str] = None,
    golden_path: Optional[Path] = None,
    n_results: int = EVAL_K,
) -> Dict:
    """
    Run the full hermetic retrieval evaluation.

    Builds the golden corpus in a temporary LanceDB (or caller-provided
    db_path), executes every golden query through VectorDB.search(), scores
    results with the shared metrics module, aggregates per class and overall.

    Args:
        report_path: Where to write the JSON report; None skips writing
            (library/gate usage). CLI passes its --report value.
        db_path: Override the throwaway store location (caller then owns
            cleanup); None creates and cleans a private temp dir.
        golden_path: Override the golden queries file.
        n_results: Window size k for recall/ndcg/fallout (MRR is uncut).

    Returns:
        Report dict (also written as JSON when report_path is given)
    """
    golden = load_golden_queries(golden_path)
    all_queries = iter_class_queries(golden)

    cleanup_dir: Optional[str] = None
    if db_path is None:
        cleanup_dir = tempfile.mkdtemp(prefix="openmem-eval-")
        db_path = cleanup_dir

    # Ground truth for validation/reporting comes from the canonical specs;
    # build_golden_corpus already verified every row landed under its exact id.
    corpus_specs = golden_corpus_specs()
    db = None
    per_query: List[Dict] = []
    try:
        db = build_golden_corpus(db_path)
        mode, capabilities = detect_retrieval_mode(db)

        # Golden-integrity gate for negatives (substring-disjointness).
        _validate_negative_disjointness(
            [s["content"] for s in corpus_specs],
            [q for cname, q in all_queries if cname == "negative"],
        )

        for class_name, q in all_queries:
            results = db.search(q["query"], n_results=n_results)
            ranked_ids = [r.get("id") for r in results]
            scores = [float(r.get("score") or 0.0) for r in results]
            relevant = list(q.get("relevant_ids", []))
            metrics = evaluate_ranked_query(ranked_ids, relevant, k=n_results)
            per_query.append({
                "id": q["id"],
                "class": class_name,
                "query": q["query"],
                "relevant_ids": relevant,
                "ranked_ids": ranked_ids,
                "scores": scores,
                "num_results": len(results),
                **metrics,
            })
    finally:
        if db is not None:
            try:
                db.close()
            except Exception:
                pass
        if cleanup_dir is not None:
            shutil.rmtree(cleanup_dir, ignore_errors=True)

    per_class: Dict[str, Dict] = {}
    metric_keys = [
        f"recall_at_{n_results}", "mrr",
        f"ndcg_at_{n_results}", f"fallout_at_{n_results}",
    ]
    for class_name in golden.get("classes", {}).keys():
        rows = [p for p in per_query if p["class"] == class_name]
        entry = {
            "queries": len(rows),
            **{key: round(_mean([p[key] for p in rows]), 4) for key in metric_keys},
        }
        if class_name == "negative":
            # The number that actually matters for distractors: how often a
            # query that should return nothing pulled ANY row into the window.
            entry["any_hit_rate"] = round(
                _mean([1.0 if p["num_results"] > 0 else 0.0 for p in rows]), 4
            )
        per_class[class_name] = entry

    aggregate = {
        "queries": len(per_query),
        **{key: round(_mean([p[key] for p in per_query]), 4) for key in metric_keys},
    }

    report = {
        "schema": "openmem.eval.report/v1",
        "generated_at": datetime.now().isoformat(),
        "golden_version": golden.get("version"),
        "corpus_version": GOLDEN_CORPUS_VERSION,
        "retrieval_mode": mode,
        "capabilities": capabilities,
        "k": n_results,
        "corpus_size": len(corpus_specs) if corpus_specs else 0,
        "per_query": per_query,
        "per_class": per_class,
        "aggregate": aggregate,
    }

    if report_path is not None:
        report_path = Path(report_path)
        report_path.parent.mkdir(parents=True, exist_ok=True)
        with open(report_path, "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2)

    return report


def render_markdown_report(report: Dict) -> str:
    """
    Render the aggregate markdown table + environment header for a report.

    Args:
        report: Report dict produced by run_eval()

    Returns:
        Markdown string safe to print to any console (ASCII only)
    """
    k = report.get("k", EVAL_K)
    caps = report.get("capabilities", {})
    lines: List[str] = []
    lines.append(f"### OpenMem Retrieval Eval - mode: {report.get('retrieval_mode')}")
    lines.append("")
    lines.append(
        f"- golden queries v{report.get('golden_version')} | corpus v"
        f"{report.get('corpus_version')} ({report.get('corpus_size')} fixtures)"
    )
    lines.append(
        f"- embedder available: {caps.get('embedder_available')} | "
        f"reranker installed: {caps.get('reranker_available')}, "
        f"loaded: {caps.get('reranker_loaded')} | gpu: {caps.get('gpu_enabled')}"
    )
    lines.append(f"- generated: {report.get('generated_at')}")
    lines.append("")
    lines.append(f"| class | queries | recall@{k} | MRR | nDCG@{k} | fallout@{k} |")
    lines.append("|---|---:|---:|---:|---:|---:|")

    def _fmt(value) -> str:
        return f"{value:.3f}" if isinstance(value, (int, float)) else str(value)

    for class_name, stats in report.get("per_class", {}).items():
        extra = (
            f" (any-hit {stats['any_hit_rate']:.3f})"
            if "any_hit_rate" in stats else ""
        )
        lines.append(
            f"| {class_name}{extra} | {stats['queries']} | "
            f"{_fmt(stats[f'recall_at_{k}'])} | {_fmt(stats['mrr'])} | "
            f"{_fmt(stats[f'ndcg_at_{k}'])} | {_fmt(stats[f'fallout_at_{k}'])} |"
        )

    agg = report.get("aggregate", {})
    lines.append(
        f"| **aggregate** | {agg.get('queries', 0)} | "
        f"{_fmt(agg.get(f'recall_at_{k}', 0.0))} | {_fmt(agg.get('mrr', 0.0))} | "
        f"{_fmt(agg.get(f'ndcg_at_{k}', 0.0))} | {_fmt(agg.get(f'fallout_at_{k}', 0.0))} |"
    )
    lines.append("")
    lines.append(
        f"Negative-class note: recall/MRR/nDCG are trivially 0.0 there "
        f"(no relevant docs exist); fallout@{k} = mean share of the top-{k} "
        f"window occupied by noise, and any-hit rate = fraction of negative "
        f"queries returning at least one row."
    )
    return "\n".join(lines)


def main(argv: Optional[List[str]] = None) -> int:
    """CLI entry point: run eval, write report, print markdown table."""
    parser = argparse.ArgumentParser(
        prog="python -m eval.run_eval",
        description="Run the hermetic OpenMem retrieval-quality evaluation",
    )
    parser.add_argument(
        "--report", default=str(DEFAULT_REPORT_PATH),
        help=f"Report JSON path (default: {DEFAULT_REPORT_PATH})",
    )
    parser.add_argument(
        "--keep-db", action="store_true",
        help="Keep the built golden store for inspection (printed on exit)",
    )
    args = parser.parse_args(argv)

    keep_dir = tempfile.mkdtemp(prefix="openmem-eval-") if args.keep_db else None
    report = run_eval(
        report_path=Path(args.report),
        db_path=keep_dir,
    )
    print(render_markdown_report(report))
    print(f"\n[Eval] Report written: {args.report}")
    if keep_dir:
        print(f"[Eval] Golden DB kept at: {keep_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
