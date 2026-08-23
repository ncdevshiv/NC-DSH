from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from moli_benchmark.synthetic import SYNTHETIC_CASES
from moli_benchmark.synthetic_matrix import (
    DEFAULT_SYNTHETIC_CONCURRENCY_MATRIX,
    FORMAL_SYNTHETIC_REPEATS,
    FORMAL_SYNTHETIC_RUNS,
    run_synthetic_matrix_suite,
)


def _synthetic_summary(cases: tuple[str, ...]) -> dict[str, object]:
    return {
        "suite": "synthetic",
        "cases": {
            case: {
                "elapsed_ms": {"p50": 100.0, "p90": 100.0, "p95": 100.0},
                "peak_pss_bytes": {"p50": 1024, "p95": 1024},
                "failures": 0,
            }
            for case in cases
        },
        "total_failures": 0,
    }


class SyntheticMatrixTests(unittest.TestCase):
    def test_formal_gate_rows_are_written_for_full_matrix(self) -> None:
        calls = []

        def run_synthetic_suite(**kwargs: object) -> dict[str, object]:
            calls.append(kwargs)
            return _synthetic_summary(tuple(kwargs["cases"]))  # type: ignore[arg-type]

        with tempfile.TemporaryDirectory() as temp_dir:
            output_dir = Path(temp_dir)
            with patch("moli_benchmark.synthetic_matrix.run_synthetic_suite", run_synthetic_suite):
                summary = run_synthetic_matrix_suite(
                    moli_bin=Path("/tmp/moli"),
                    output_dir=output_dir,
                    profile="formal",
                    runs=FORMAL_SYNTHETIC_RUNS,
                    timeout_seconds=30,
                    cases=SYNTHETIC_CASES,
                    concurrency_levels=DEFAULT_SYNTHETIC_CONCURRENCY_MATRIX,
                    repeats=FORMAL_SYNTHETIC_REPEATS,
                    stability_threshold_percent=10.0,
                )

            gate_rows = json.loads((output_dir / "synthetic-matrix" / "gate-rows.json").read_text(encoding="utf-8"))

        self.assertEqual(len(calls), len(DEFAULT_SYNTHETIC_CONCURRENCY_MATRIX) * FORMAL_SYNTHETIC_REPEATS)
        self.assertEqual(summary["gate_failures"], 0)
        self.assertTrue(all(row["ok"] for row in summary["formal_gate_rows"]))
        self.assertEqual(gate_rows["rows"], summary["formal_gate_rows"])

    def test_formal_gate_rows_identify_profile_requirement_failures(self) -> None:
        def run_synthetic_suite(**kwargs: object) -> dict[str, object]:
            return _synthetic_summary(tuple(kwargs["cases"]))  # type: ignore[arg-type]

        with tempfile.TemporaryDirectory() as temp_dir:
            with patch("moli_benchmark.synthetic_matrix.run_synthetic_suite", run_synthetic_suite):
                summary = run_synthetic_matrix_suite(
                    moli_bin=Path("/tmp/moli"),
                    output_dir=Path(temp_dir),
                    profile="formal",
                    runs=1,
                    timeout_seconds=30,
                    cases=("static-html",),
                    concurrency_levels=(1,),
                    repeats=1,
                    stability_threshold_percent=10.0,
                )

        by_gate = {row["gate"]: row for row in summary["formal_gate_rows"]}
        self.assertEqual(by_gate["runs"]["failure_kind"], "formal-requirement")
        self.assertEqual(by_gate["repeats"]["failure_kind"], "formal-requirement")
        self.assertEqual(by_gate["concurrency_levels"]["failure_kind"], "formal-requirement")
        self.assertEqual(by_gate["cases"]["failure_kind"], "formal-requirement")
        self.assertGreater(summary["gate_failures"], 0)


if __name__ == "__main__":
    unittest.main()
