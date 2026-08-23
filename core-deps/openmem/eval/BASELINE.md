# Retrieval-Quality Baseline — OpenMem Phase 3

First measured baseline of the golden retrieval eval. This file is the
reference for every threshold asserted by `tests/test_retrieval_gate.py`.

## Measurement environment

- **Date**: 2026-08-22
- **Interpreter**: F:\openmem\.venv\Scripts\python.exe (Python 3.12.14)
- **Storage**: lancedb 0.37.1 / pyarrow 25, fixed-size vector schema
- **Retrieval mode at measurement time**: `keyword-fallback`
  (sentence-transformers NOT installed → `VectorDB.search()` routes to
  `_keyword_search`: case-insensitive per-term substring OR-match, score =
  fraction of distinct query terms matched, ranked by distinct-term hits
  then total term frequency, stable sort over insertion order)
- **Reranker**: not installed (`BAAI/bge-reranker-*` unavailable), no GPU
- **Corpus**: hermetic, deterministic — 36 fixtures across 6 topics
  (deepseek harness port, nc-code repo, retrieval internals, plus
  gardening/baking/travel distractors), explicit `golden-*` ids, built in a
  TEMP LanceDB via the standard constructor; live `data/lancedb` untouched.
- **Queries**: `eval/golden_queries.json` v1.0.0 — 16 queries: 6 exact-term,
  6 paraphrase, 4 negative/distractor.

## Measured results (verbatim from first run)

```
### OpenMem Retrieval Eval - mode: keyword-fallback

- golden queries v1.0.0 | corpus v1.0.0 (36 fixtures)
- embedder available: False | reranker installed: False, loaded: False | gpu: False

| class | queries | recall@5 | MRR | nDCG@5 | fallout@5 |
|---|---:|---:|---:|---:|---:|
| exact_term | 6 | 0.972 | 1.000 | 1.000 | 0.333 |
| paraphrase | 6 | 1.000 | 0.889 | 0.917 | 0.300 |
| negative (any-hit 0.000) | 4 | 0.000 | 0.000 | 0.000 | 0.000 |
| **aggregate** | 16 | 0.740 | 0.708 | 0.719 | 0.237 |
```

Metric conventions are defined in `memory_store/retrieval_metrics.py`.
Negative-class recall/MRR/nDCG are trivially 0.0 because no relevant docs
exist for those queries; `fallout@5` (mean share of the top-5 window that
is non-relevant) and any-hit rate are the numbers that matter there.

## Chosen gate thresholds and rationale

The corpus, ids, contents, and ranking path are fully deterministic
(stable sort over per-term hit counts; no embedder, no network), so an
unchanged system reproduces these numbers exactly. Thresholds therefore do
NOT budget statistical noise — they exist to trip when *ranking semantics*
drift (scoring formula changes, sort-order changes, schema/search rewrites,
a future vector-mode switch). Floors are set ~10–20% below observed;
fallout ceilings ~10–20% above observed; negatives stay strict-zero
because the runner validates negative-query terms as substring-disjoint
from the corpus at runtime, making silence enforceable rather than lucky.

| Gate assertion | Observed | Threshold | Slack |
|---|---:|---:|---|
| exact_term recall@5 ≥ | 0.972 | **0.87** | −10% |
| exact_term MRR ≥ | 1.000 | **0.95** | −5% |
| exact_term nDCG@5 ≥ | 1.000 | **0.90** | −10% |
| exact_term fallout@5 ≤ | 0.333 | **0.40** | +20% |
| paraphrase recall@5 ≥ | 1.000 | **0.90** | −10% |
| paraphrase MRR ≥ | 0.889 | **0.75** | −16% |
| paraphrase nDCG@5 ≥ | 0.917 | **0.80** | −13% |
| paraphrase fallout@5 ≤ | 0.300 | **0.36** | +20% |
| negative any-hit rate ≤ | 0.000 | **0.00** | strict |
| negative fallout@5 ≤ | 0.000 | **0.02** | strict |
| aggregate recall@5 ≥ | 0.740 | **0.65** | −12% |
| aggregate MRR ≥ | 0.708 | **0.60** | −15% |
| aggregate nDCG@5 ≥ | 0.719 | **0.62** | −14% |
| aggregate fallout@5 ≤ | 0.237 | **0.28** | +18% |

Notes on interpretation:

- exact_term recall@5 is structurally capped below 1.0: query ex-01 judges
  all six harness fixtures relevant, so its ceiling is 5/6 ≈ 0.833.
- aggregate recall/MRR average across ALL queries including negatives,
  whose recall/MRR are honest zeros by construction.
- Positive-class fallout > 0 is expected: tight relevance judgments mean
  topically-correct-but-not-judged docs count as false discoveries within
  the window.

## Reproduce

```bash
python main.py eval                       # writes data/eval/latest.json
python -m eval.run_eval --report out.json # same thing, library entry too
python -m unittest tests.test_retrieval_gate -v   # regression gate
```

## Search quirks exposed by this eval (findings — not fixed)

1. **Substring morphological collisions** drive real fallout:
   - `'port'` matches *Ported* AND *re**port***, *im**portance***,
     *pass**port***, *Air**port*** (pa-01 pulls travel/harness docs).
   - `'repo'` matches *report* — nc-code query ex-03 surfaced a DeepSeek
     harness doc ("…jobs that **report** job ids") inside its top-5.
2. **Polysemy across domains**: `'scoring'` matches bread-scoring
   (`golden-bak-05`) just like search-scoring (`golden-ret-02`); ex-05's
   runner-up is literally about crust.
3. **Tie-breaks are insertion order**: single-shared-term paraphrase pa-06
   ("keep travel documents dry") loses to earlier-inserted docs matching
   only one equally-weighted term ('keep' ⊂ "OpenMem keeps…", "keep cane
   borers away"), dropping MRR to 0.333 for that query — term frequency
   tie-break only fires after distinct-hit counts, which are equal (1 vs 1).
4. **Frequency tie-break favors repetition**: ex-01 ranks `golden-dsh-06`
   first purely because it repeats 'harness' twice.
5. Any future reranker/embedder work should target exactly these three
   levers: morphology-aware matching, IDF-style term weighting, and a
   semantic tie-break.

When a deliberate retrieval improvement lands here, re-measure
(`python main.py eval`), update the table above, and tighten or loosen the
gate thresholds with a one-line justification each — never silently.
