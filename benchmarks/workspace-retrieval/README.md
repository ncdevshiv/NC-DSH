# Workspace retrieval benchmarks

English | [中文](README.zh.md)

## Summary

One required lane compares the shipped zvec provider with in-process lexical and brute-force vector references over the same synthetic corpus. It reports retrieval quality ratios, token cost per query, and measured wall/CPU time for indexing, lexical, semantic, hybrid, and close. No case reads a repository, a recorded Session, user material, or a network service.

## Table of Contents

- [Run](#run)
- [Measurements](#measurements)
- [Dev Note](#dev-note)

<a id="run"></a>

## Run

From the repository root, build benchmarks with `bun run build:bench`, then run `bun exec vitest run --config vitest.bench.config.ts benchmarks/workspace-retrieval/workspace-retrieval.bench.ts`. Do not overlap timing runs with builds or other benchmarks.

The worker runs under plain Node from `benchmarks/.dsh-build/` and resolves workspace packages through their built `lib/` entries. It creates a private temporary workspace, indexes the corpus, and removes the root after the run.

<a id="measurements"></a>

## Measurements

[corpus.ts](corpus.ts) owns the fixed corpus. 2,000 documents of 80 words each contain either a rare lexical term (25 documents per term, never in filler) or a category vector whose text deliberately omits the semantic query word. Ten lexical queries and eight semantic queries exercise both modes; every document carries a dense vector so semantic and hybrid runs use the same corpus.

| Row | Comparison |
|---|---|
| Lexical | zvec native FTS vs. an exact substring scan over the same documents |
| Semantic | zvec FLAT cosine search vs. brute-force cosine over the same vectors |
| Hybrid | zvec RRF fusion vs. in-process RRF over the two reference lists |
| Tokens | `dsh-token-meter/estimate` price of the returned snippets on both sides |
| Time | Wall and process CPU milliseconds for indexing, per-query modes, and close |

Quality assertions require zvec to match the reference recall, precision, and reciprocal rank within the tolerance recorded in the benchmark file; token rows must stay within the recorded ratio. Timing budgets use the shared CI calibration.

<a id="dev-note"></a>

## Dev Note

None.
