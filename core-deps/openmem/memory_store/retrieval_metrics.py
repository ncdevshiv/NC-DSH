"""
Retrieval quality metrics for OpenMem.

Pure, dependency-free IR metrics used by eval/run_eval.py and
tests/test_retrieval_gate.py to make retrieval-quality claims provable by
number instead of adjectives.

Conventions (apply to every function in this module):

- Binary relevance: an id is relevant iff it is a member of ``relevant_ids``.
  Graded relevance is deliberately not modeled.
- ``ranked_ids`` is a list of retrieved memory ids in rank order (rank 1 =
  first element). Duplicate ids are tolerated; a duplicated relevant id is
  credited once (recall) and scored at its best (earliest) position (MRR,
  nDCG).
- Empty cases are honest zeros: no relevant ids, an empty ranking, or a
  non-positive ``k`` all return 0.0 rather than raising or returning NaN,
  so aggregate averages never need special-casing.
- nDCG uses the standard binary-relevance form:
  DCG@k   = sum over positions i=1..k of rel_i / log2(i + 1)
  IDCG@k  = sum over positions i=1..min(len(relevant), k) of 1 / log2(i + 1)
- fallout@k is the false-discovery rate inside the retrieval window:
  (# of top-k results that are NOT relevant) / k. The denominator is the
  fixed window size k, so short result lists score lower noise density and
  a negative query with zero hits scores exactly 0.0.
"""

import math
from typing import Iterable, List, Optional, Set


def _relevant_set(relevant_ids: Optional[Iterable[str]]) -> Set[str]:
    """Normalize a relevance judgment into a set (None -> empty set)."""
    if relevant_ids is None:
        return set()
    return {str(r) for r in relevant_ids}


def _top_k_unique(ranked_ids: List[str], k: int) -> List[str]:
    """First k entries of ranked_ids with duplicates collapsed to first occurrence."""
    seen: Set[str] = set()
    out: List[str] = []
    for mid in ranked_ids[:max(0, int(k))]:
        if mid not in seen:
            seen.add(mid)
            out.append(mid)
    return out


def recall_at_k(ranked_ids: List[str], relevant_ids: Iterable[str], k: int) -> float:
    """
    Fraction of relevant documents retrieved within the top k ranks.

    Args:
        ranked_ids: Retrieved memory ids in rank order
        relevant_ids: Ids judged relevant for the query
        k: Cutoff rank (positions beyond k are ignored)

    Returns:
        |top-k unique ∩ relevant| / |relevant|; 0.0 when there are no
        relevant ids, an empty ranking, or k <= 0
    """
    relevant = _relevant_set(relevant_ids)
    if not relevant or k <= 0 or not ranked_ids:
        return 0.0

    top_k = _top_k_unique(ranked_ids, k)
    hits = sum(1 for mid in top_k if mid in relevant)
    return hits / len(relevant)


def mrr(ranked_ids: List[str], relevant_ids: Iterable[str]) -> float:
    """
    Mean-reciprocal-rank contribution for a single query: 1 / rank of the
    first relevant document in the full ranking (no cutoff).

    Args:
        ranked_ids: Retrieved memory ids in rank order
        relevant_ids: Ids judged relevant for the query

    Returns:
        1/rank of the first relevant id; 0.0 when nothing relevant appears
        or the ranking is empty
    """
    relevant = _relevant_set(relevant_ids)
    if not relevant or not ranked_ids:
        return 0.0

    seen: Set[str] = set()
    for rank, mid in enumerate(ranked_ids, start=1):
        if mid in seen:
            continue
        seen.add(mid)
        if str(mid) in relevant:
            return 1.0 / rank
    return 0.0


def ndcg_at_k(ranked_ids: List[str], relevant_ids: Iterable[str], k: int) -> float:
    """
    Normalized discounted cumulative gain at k under binary relevance.

    DCG credits each relevant hit at rank i with 1 / log2(i + 1); the ideal
    DCG places min(len(relevant), k) hits at the top of the ranking, which
    makes 1.0 achievable whenever k >= len(relevant).

    Args:
        ranked_ids: Retrieved memory ids in rank order
        relevant_ids: Ids judged relevant for the query
        k: Cutoff rank

    Returns:
        DCG@k / IDCG@k in [0.0, 1.0]; 0.0 when there are no relevant ids,
        an empty ranking, or k <= 0
    """
    relevant = _relevant_set(relevant_ids)
    if not relevant or k <= 0 or not ranked_ids:
        return 0.0

    top_k = _top_k_unique(ranked_ids, k)

    dcg = sum(
        1.0 / math.log2(i + 1)
        for i, mid in enumerate(top_k, start=1)
        if str(mid) in relevant
    )
    ideal_depth = min(len(relevant), int(k))
    idcg = sum(1.0 / math.log2(i + 1) for i in range(1, ideal_depth + 1))
    if idcg == 0.0:
        return 0.0
    return dcg / idcg


def fallout_at_k(ranked_ids: List[str], relevant_ids: Iterable[str], k: int) -> float:
    """
    False-discovery rate inside the top-k window.

    Counts non-relevant documents retrieved in the top k, divided by the
    fixed window size k. For negative/distractor queries (``relevant_ids``
    empty) every retrieved row is a false positive, so this reduces to
    len(results)/k -- the direct measure of how much noise a query that
    should return nothing actually pulled in.

    Args:
        ranked_ids: Retrieved memory ids in rank order
        relevant_ids: Ids judged relevant for the query (may be empty)
        k: Cutoff rank (fixed denominator)

    Returns:
        (# top-k results not in relevant) / k, clamped to [0.0, 1.0]
    """
    if k <= 0:
        return 0.0
    relevant = _relevant_set(relevant_ids)
    window = ranked_ids[:int(k)]
    misses = sum(1 for mid in window if str(mid) not in relevant)
    return min(misses / float(k), 1.0)


def evaluate_ranked_query(
    ranked_ids: List[str],
    relevant_ids: Iterable[str],
    k: int = 5,
) -> dict:
    """
    Compute every metric in this module for one ranked result list.

    Convenience wrapper used by the eval runner so per-query records carry
    a consistent metric block.

    Args:
        ranked_ids: Retrieved memory ids in rank order
        relevant_ids: Ids judged relevant for the query (may be empty)
        k: Cutoff for recall/ndcg/fallout (MRR is uncut)

    Returns:
        Dict with keys: recall_at_k, mrr, ndcg_at_k, fallout_at_k
    """
    return {
        f"recall_at_{k}": recall_at_k(ranked_ids, relevant_ids, k),
        "mrr": mrr(ranked_ids, relevant_ids),
        f"ndcg_at_{k}": ndcg_at_k(ranked_ids, relevant_ids, k),
        f"fallout_at_{k}": fallout_at_k(ranked_ids, relevant_ids, k),
    }
