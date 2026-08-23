"""
Retrieval-quality regression gate.

Runs the full hermetic eval in-process (temp corpus, temp store, temp
report -- never data/lancedb) and asserts documented metric floors/ceilings
chosen from the FIRST measured baseline (see eval/BASELINE.md):

- The corpus and query set are fully deterministic and keyword-fallback
  ranking is a stable sort over per-term hit counts, so unchanged code
  reproduces baseline numbers exactly. Thresholds are therefore set
  ~10-20% below observed values (fallout: above) -- tight enough that real
  ranking regressions trip, loose enough that benign tie-order noise never
  does.
- Negative queries must stay perfectly silent (any_hit_rate == 0.0): the
  runner validates substring-disjointness of negative terms against the
  corpus at runtime, so any nonzero value means corpus/query drift and is
  a genuine gate failure.

On failure every assertion message embeds the full markdown metrics table,
so CI logs show actual numbers without re-running anything.
"""

import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from eval.run_eval import render_markdown_report, run_eval

# Baseline: first measured run, 2026-08-22, keyword-fallback mode, no
# reranker, no embedder (eval/BASELINE.md). Floors sit ~10-20% under the
# observed value; fallout ceilings sit ~10-20% above it.
THRESHOLDS = {
    "exact_term": {
        "recall_at_5_min": 0.87,   # observed 0.972 (-10%)
        "mrr_min": 0.95,           # observed 1.000
        "ndcg_at_5_min": 0.90,     # observed 1.000
        "fallout_at_5_max": 0.40,  # observed 0.333 (+20%)
    },
    "paraphrase": {
        "recall_at_5_min": 0.90,   # observed 1.000
        "mrr_min": 0.75,           # observed 0.889 (-16%)
        "ndcg_at_5_min": 0.80,     # observed 0.917 (-13%)
        "fallout_at_5_max": 0.36,  # observed 0.300 (+20%)
    },
    "negative": {
        "recall_at_5_max": 0.02,   # observed 0.000 -- negatives stay silent
        "fallout_at_5_max": 0.02,  # observed 0.000
        "any_hit_rate_max": 0.0,   # observed 0.000 -- strict by construction
    },
    "aggregate": {
        "recall_at_5_min": 0.65,   # observed 0.740 (-12%)
        "mrr_min": 0.60,           # observed 0.708 (-15%)
        "ndcg_at_5_min": 0.62,     # observed 0.719 (-14%)
        "fallout_at_5_max": 0.28,  # observed 0.237 (+18%)
    },
}


class TestRetrievalQualityGate(unittest.TestCase):
    """Hermetic end-to-end retrieval gate (no live-store contact)."""

    @classmethod
    def setUpClass(cls):
        cls.test_dir = tempfile.mkdtemp(prefix="openmem-gate-")
        cls.db_path = os.path.join(cls.test_dir, "golden_lancedb")
        cls.report_path = os.path.join(cls.test_dir, "gate_report.json")
        # One shared in-process run: corpus build dominates cost, and every
        # assertion below reads the same measured report.
        cls.report = run_eval(
            report_path=Path(cls.report_path),
            db_path=cls.db_path,
        )
        cls.table = render_markdown_report(cls.report)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.test_dir, ignore_errors=True)

    def _class(self, name):
        return self.report["per_class"][name]

    def _floor_check(self, label, actual, floor):
        """Assert metric >= floor with the full table attached on failure."""
        self.assertGreaterEqual(
            actual, floor,
            f"\nGATE FAILURE {label}: actual {actual:.4f} < floor {floor}"
            f"\n\n{self.table}\n",
        )

    def _ceiling_check(self, label, actual, ceiling):
        """Assert metric <= ceiling with the full table attached on failure."""
        self.assertLessEqual(
            actual, ceiling,
            f"\nGATE FAILURE {label}: actual {actual:.4f} > ceiling {ceiling}"
            f"\n\n{self.table}\n",
        )

    def test_exact_term_class_meets_baseline_thresholds(self):
        stats = self._class("exact_term")
        t = THRESHOLDS["exact_term"]
        self._floor_check("exact_term.recall@5", stats["recall_at_5"], t["recall_at_5_min"])
        self._floor_check("exact_term.mrr", stats["mrr"], t["mrr_min"])
        self._floor_check("exact_term.ndcg@5", stats["ndcg_at_5"], t["ndcg_at_5_min"])
        self._ceiling_check("exact_term.fallout@5", stats["fallout_at_5"], t["fallout_at_5_max"])

    def test_paraphrase_class_meets_baseline_thresholds(self):
        stats = self._class("paraphrase")
        t = THRESHOLDS["paraphrase"]
        self._floor_check("paraphrase.recall@5", stats["recall_at_5"], t["recall_at_5_min"])
        self._floor_check("paraphrase.mrr", stats["mrr"], t["mrr_min"])
        self._floor_check("paraphrase.ndcg@5", stats["ndcg_at_5"], t["ndcg_at_5_min"])
        self._ceiling_check("paraphrase.fallout@5", stats["fallout_at_5"], t["fallout_at_5_max"])

    def test_negative_queries_stay_silent(self):
        stats = self._class("negative")
        t = THRESHOLDS["negative"]
        self._ceiling_check(
            "negative.any_hit_rate", stats["any_hit_rate"], t["any_hit_rate_max"]
        )
        self._ceiling_check("negative.fallout@5", stats["fallout_at_5"], t["fallout_at_5_max"])

    def test_aggregate_meets_baseline_thresholds(self):
        agg = self.report["aggregate"]
        t = THRESHOLDS["aggregate"]
        self._floor_check("aggregate.recall@5", agg["recall_at_5"], t["recall_at_5_min"])
        self._floor_check("aggregate.mrr", agg["mrr"], t["mrr_min"])
        self._floor_check("aggregate.ndcg@5", agg["ndcg_at_5"], t["ndcg_at_5_min"])
        self._ceiling_check("aggregate.fallout@5", agg["fallout_at_5"], t["fallout_at_5_max"])

    def test_mode_is_reported_honestly(self):
        """The reported mode must match the store's real capabilities."""
        caps = self.report["capabilities"]
        mode = self.report["retrieval_mode"]
        if caps["embedder_available"]:
            self.assertEqual(mode, "vector")
        else:
            self.assertEqual(
                mode, "keyword-fallback",
                f"\n{self.table}\n",
            )
        self.assertIn(mode, {"keyword-fallback", "vector"})
        self.assertFalse(caps["reranker_loaded"] and not caps["reranker_available"])

    def test_report_json_written_and_round_trips(self):
        with open(self.report_path, "r", encoding="utf-8") as f:
            loaded = json.load(f)
        self.assertEqual(loaded["schema"], "openmem.eval.report/v1")
        self.assertEqual(loaded["retrieval_mode"], self.report["retrieval_mode"])
        self.assertEqual(len(loaded["per_query"]), len(self.report["per_query"]))
        # Every per-query record carries the full metric block.
        for q in loaded["per_query"]:
            for key in ("recall_at_5", "mrr", "ndcg_at_5", "fallout_at_5"):
                self.assertIn(key, q)
                self.assertIsInstance(q[key], float)


class TestEvalDeterminism(unittest.TestCase):
    """Two independent builds of the corpus must score identically."""

    def test_repeated_runs_produce_identical_metrics(self):
        first = run_eval(report_path=None)
        second = run_eval(report_path=None)
        self.assertEqual(first["per_class"], second["per_class"])
        self.assertEqual(first["aggregate"], second["aggregate"])
        self.assertEqual(
            [q["ranked_ids"] for q in first["per_query"]],
            [q["ranked_ids"] for q in second["per_query"]],
        )


if __name__ == "__main__":
    unittest.main()
