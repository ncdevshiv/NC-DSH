# ai-stream Mutation Testing Pilot

**Status: REAL RUN COMPLETED** — `cargo-mutants v27.1.0` installed cleanly and the
scoped run finished **within budget (9m wall, cap was ~10m)** on Windows,
rustc 1.97.1. No `.mutants.toml` fallback was needed.

## How to reproduce

```sh
cargo install cargo-mutants --locked
cargo mutants -p ai-stream --timeout 60 --colors never   # from workspace root
```

Results land in `mutants.out/` (scratch copy — the source tree stays
untouched; verified via `git status`: zero tracked-file modifications from
the run). Delete `mutants.out/` afterwards to keep the worktree tidy.

## Results (run date: this pilot)

| Metric            | Count | Notes                                            |
|-------------------|------:|--------------------------------------------------|
| Total mutants     |    71 |                                                  |
| Caught (killed)   |    50 | test-suite failures                              |
| **Missed**        | **10** | survive the entire suite — see table below      |
| Timed out         |     6 | mutant hangs the suite (detected, 60 s each)     |
| Unviable          |     5 | mutant does not compile                          |

**Kill rate: 50 / 60 = 83.3 %** (caught ÷ [caught + missed]).
Treating the 6 timeouts as detections (the suite *did* flag them, by hang):
56 / 60 = **93.3 %**.

All 6 timeouts cluster in `find_line_end` / the `sse_parse` scan loop
(`start += …` mutations) — arithmetic corruption causes an infinite scan.
These are effectively caught-by-hang; a cheap mitigation for future runs is
a lower `--timeout` plus a property test asserting forward progress.

## Surviving mutants worth fixing

Every survivor lives in `crates/ai-stream/src/lib.rs`. Root cause pattern:
**the public API is tested through happy paths whose outputs are supplied by
the caller** (e.g. `ToolCallCompleted` carries the finished call), so
assembly logic can be deleted without failing anything.

| # | Location | Mutation | Why it survives | Killing test to add |
|---|----------|----------|-----------------|---------------------|
| 1 | `strip_line_end` 74:27 | `==` → `!=` (inner CR check) | Branch is unreachable via `sse_parse` (`find_line_end` always consumes `\r\n` as one terminator) | Make `strip_line_end` `pub(crate)` and unit-test `"a\r\n"` / `"a\n"` / `"a\r"` directly, or delete the dead branch |
| 2 | `strip_line_end` 75:28 | `-` → `/` (slice arithmetic) | Same dead-branch reachability gap | Same as #1 |
| 3 | `sse_parse` 226:24 | delete `!` (`if !buffer.is_empty()`) | On EOF-with-buffer, mutated code calls `feed_line("")`, whose empty line *also* dispatches — accidental equivalence for inputs ending in `\n` | Feed `"data: tail"` with **no trailing newline**: mutant loses the final event |
| 4 | `collect_text` 252 | delete `StreamEvent::Error` arm | No unit test asserts `collect_text` maps an in-band error event to `Err` | Unit test: stream containing `Error { message }` must yield `Err` |
| 5 | `ToolCallAccumulator::push` 280 | delete `ToolCallStarted` arm | Existing test completes via `ToolCallCompleted`, which carries the full call — insertion is never observed | Start + deltas + `finalize()`; assert name/arguments |
| 6 | `ToolCallAccumulator::push` 284 | delete `ToolCallDelta` arm | Same: `Completed` bypasses argument accumulation | Start + deltas + `finalize_and_drain()` (no `Completed`); assert assembled JSON |
| 7 | `finalize` 310 | body → `()` | Nothing exercises the in-flight→completed promotion | Test from #5 kills this too |
| 8 | `finalize_and_drain` 327 | return → `vec![]` | Function unexercised inside `-p ai-stream` tests (only external adapters call it) | Direct unit test: started+delta → `finalize_and_drain()` returns exactly one call |
| 9 | `drain_completed` 334 | return → `vec![]` | Unexercised | Unit test: two batches drained return disjoint sets; second drain is empty |
| 10 | `collect_completion` 360 | delete `StreamEvent::Error` arm | No unit test asserts error propagation through aggregation | Unit test mirroring #4 against `collect_completion` |

Estimated impact: adding ~6 focused unit tests (error-event propagation ×2,
accumulator lifecycle ×3, unterminated-final-line ×1) would raise the kill
rate above 90 % strict.

## Notes

- cargo-mutants worked entirely in `mutants.out/` scratch; `git status`
  confirmed no tracked files were touched by the mutation run.
- The 60 s per-mutant timeout dominated wall time (6 × 60 s ≈ half the
  budget). Tighten to `--timeout 30` for repeat runs of this package.
