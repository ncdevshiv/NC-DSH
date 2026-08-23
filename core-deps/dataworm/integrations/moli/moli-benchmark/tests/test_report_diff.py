from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from moli_benchmark.artifacts import write_json
from moli_benchmark.report_diff import build_report_diff, load_baseline_summary


class ReportDiffTests(unittest.TestCase):
    def test_build_report_diff_tracks_suite_changes(self) -> None:
        baseline = {
            "suites": [
                {"suite": "startup", "total_failures": 2, "cases": 1},
                {"suite": "wpt", "total_failures": 0, "gate_failures": 0, "cases": 10},
                {"suite": "wild-web", "gate_failures": 1, "cases": ["baidu-home"]},
            ]
        }
        diff = build_report_diff(
            current_summaries=[
                {"suite": "startup", "total_failures": 1, "cases": 1},
                {"suite": "wpt", "total_failures": 0, "gate_failures": 0, "cases": 10},
                {"suite": "synthetic", "total_failures": 0, "cases": {"static-html": {}}},
            ],
            baseline_summary=baseline,
            baseline_path=Path("/previous/summary.json"),
        )

        by_suite = {row["suite"]: row for row in diff["suites"]}
        self.assertEqual(by_suite["startup"]["status"], "changed")
        self.assertEqual(by_suite["startup"]["total_failures_delta"], -1)
        self.assertEqual(by_suite["wpt"]["status"], "unchanged")
        self.assertEqual(by_suite["wild-web"]["status"], "removed")
        self.assertEqual(by_suite["synthetic"]["status"], "added")
        self.assertEqual(diff["summary"]["added"], 1)
        self.assertEqual(diff["summary"]["removed"], 1)
        self.assertEqual(diff["summary"]["changed"], 1)
        self.assertEqual(diff["summary"]["total_failures_delta"], -1)
        self.assertEqual(diff["summary"]["gate_failures_delta"], -2)

    def test_load_baseline_summary_accepts_report_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            report_dir = Path(temp_dir)
            write_json(report_dir / "summary.json", {"suites": [{"suite": "startup"}]})

            self.assertEqual(load_baseline_summary(report_dir)["suites"][0]["suite"], "startup")

    def test_load_baseline_summary_rejects_missing_summary(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            with self.assertRaisesRegex(RuntimeError, "missing baseline report summary"):
                load_baseline_summary(Path(temp_dir))


if __name__ == "__main__":
    unittest.main()
