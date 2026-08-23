"""
Unit tests for memory_store.retrieval_metrics.

Every assertion below is hand-computed, not derived from the
implementation, so a regression in the metric math cannot hide behind its
own definition.
"""

import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from memory_store.retrieval_metrics import (
    evaluate_ranked_query,
    fallout_at_k,
    mrr,
    ndcg_at_k,
    recall_at_k,
)


class TestRecallAtK(unittest.TestCase):
    """recall@k against hand-computed fractions."""

    def test_partial_overlap_one_of_three_relevant_in_top3(self):
        # top-3 = [a, b, c]; relevant = {b, d, e}; hit = {b} -> 1/3
        self.assertAlmostEqual(
            recall_at_k(["a", "b", "c", "d", "e"], {"b", "d", "e"}, 3),
            1.0 / 3.0,
        )

    def test_perfect_recall_when_all_relevant_ranked_top(self):
        self.assertEqual(recall_at_k(["a", "b", "c"], {"c", "a"}, 3), 1.0)

    def test_cutoff_excludes_late_hit(self):
        # d is relevant but sits at rank 4 > k=3 -> only {b} counts
        self.assertEqual(recall_at_k(["a", "b", "c", "d"], {"b", "d"}, 3), 0.5)

    def test_duplicate_relevant_id_credited_once(self):
        # 'a' appears twice in the window but must count as one hit
        self.assertEqual(recall_at_k(["a", "a", "b"], {"a", "b"}, 3), 1.0)

    def test_empty_relevant_judgment_is_zero_not_error(self):
        self.assertEqual(recall_at_k(["a", "b"], set(), 5), 0.0)
        self.assertEqual(recall_at_k(["a", "b"], None, 5), 0.0)

    def test_empty_ranking_is_zero(self):
        self.assertEqual(recall_at_k([], {"a"}, 5), 0.0)

    def test_non_positive_k_is_zero(self):
        self.assertEqual(recall_at_k(["a"], {"a"}, 0), 0.0)
        self.assertEqual(recall_at_k(["a"], {"a"}, -2), 0.0)


class TestMRR(unittest.TestCase):
    """MRR against hand-computed reciprocals."""

    def test_first_relevant_at_rank_two(self):
        # c irrelevant; a (relevant) at rank 2 -> 1/2
        self.assertEqual(mrr(["c", "a", "b"], {"a", "b"}), 0.5)

    def test_first_relevant_at_rank_one_scores_one(self):
        self.assertEqual(mrr(["x", "y"], {"x"}), 1.0)

    def test_no_relevant_result_scores_zero(self):
        self.assertEqual(mrr(["x", "y", "z"], {"q"}), 0.0)

    def test_empty_inputs_are_zero_not_error(self):
        self.assertEqual(mrr([], {"a"}), 0.0)
        self.assertEqual(mrr(["a"], set()), 0.0)

    def test_earliest_duplicate_wins(self):
        # duplicated 'a' must score at its best (first) position, not later
        self.assertEqual(mrr(["b", "a", "a"], {"a"}), 0.5)


class TestNDCGAtK(unittest.TestCase):
    """nDCG@k against hand-computed DCG/IDCG ratios."""

    def test_perfect_single_relevant_at_top(self):
        # DCG = 1/log2(2) = 1; IDCG = 1 -> 1.0
        self.assertEqual(ndcg_at_k(["a", "b", "c"], {"a"}, 3), 1.0)

    def test_single_relevant_at_last_position(self):
        # DCG = 1/log2(4) = 0.5; IDCG = min(1, 3) ideal = 1 -> exactly 0.5
        self.assertEqual(ndcg_at_k(["x", "y", "a"], {"a"}, 3), 0.5)

    def test_two_relevant_one_misplaced(self):
        # DCG = b@1: 1/log2(2) + a@3: 1/log2(4) = 1 + 0.5 = 1.5
        # IDCG = 1/log2(2) + 1/log2(3) = 1.63093...
        expected = 1.5 / (1.0 + 1.0 / math.log2(3))
        self.assertAlmostEqual(ndcg_at_k(["b", "c", "a"], {"a", "b"}, 3), expected)

    def test_more_relevant_than_k_still_reaches_one(self):
        # Ideal depth is min(len(rel), k) = 2, and both top ranks are
        # relevant, so DCG == IDCG exactly even though 2 relevant ids
        # are missing from the list.
        dcg = 1.0 + 1.0 / math.log2(3)
        self.assertAlmostEqual(ndcg_at_k(["a", "b"], {"a", "b", "c", "d"}, 2), dcg / dcg)

    def test_all_irrelevant_is_zero(self):
        self.assertEqual(ndcg_at_k(["x", "y", "z"], {"a"}, 3), 0.0)

    def test_empty_cases_are_zero_not_error(self):
        self.assertEqual(ndcg_at_k([], {"a"}, 3), 0.0)
        self.assertEqual(ndcg_at_k(["a"], set(), 3), 0.0)
        self.assertEqual(ndcg_at_k(["a"], {"a"}, 0), 0.0)

    def test_score_bounded_between_zero_and_one(self):
        score = ndcg_at_k(["x", "y", "a", "b"], {"a", "b"}, 4)
        self.assertGreaterEqual(score, 0.0)
        self.assertLessEqual(score, 1.0)


class TestFalloutAtK(unittest.TestCase):
    """fallout@k = non-relevant retrieved / fixed window size k."""

    def test_negative_query_with_full_window_is_total_noise(self):
        # No relevant ids exist; every slot is a false positive -> 3/3
        self.assertEqual(fallout_at_k(["x", "y", "z"], set(), 3), 1.0)

    def test_mixed_window_counts_only_non_relevant(self):
        # window [x, a, y], relevant {a} -> 2 misses / k=3
        self.assertAlmostEqual(fallout_at_k(["x", "a", "y"], {"a"}, 3), 2.0 / 3.0)

    def test_short_result_list_scores_lower_noise_density(self):
        # Fixed denominator k=5, one noise row -> 0.2
        self.assertEqual(fallout_at_k(["x"], set(), 5), 0.2)

    def test_negative_query_with_zero_hits_is_zero(self):
        self.assertEqual(fallout_at_k([], set(), 5), 0.0)

    def test_non_positive_k_is_zero(self):
        self.assertEqual(fallout_at_k(["x"], set(), 0), 0.0)


class TestEvaluateRankedQuery(unittest.TestCase):
    """The runner-facing wrapper returns a consistent metric block."""

    def test_block_keys_and_values(self):
        block = evaluate_ranked_query(["a", "x", "b"], {"a", "b"}, k=5)
        self.assertEqual(
            set(block.keys()),
            {"recall_at_5", "mrr", "ndcg_at_5", "fallout_at_5"},
        )
        self.assertEqual(block["mrr"], 1.0)
        # window [a, x, b] holds exactly one non-relevant id over k=5
        self.assertEqual(block["fallout_at_5"], 1.0 / 5.0)

    def test_negative_query_block_is_honest_zeros(self):
        block = evaluate_ranked_query([], set(), k=5)
        for value in block.values():
            self.assertEqual(value, 0.0)


if __name__ == "__main__":
    unittest.main()
