from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from moli_benchmark.wpt import (
    REPORT_PREFIX,
    _by_tag_rows,
    _collect_reports,
    _run_command,
    _wpt_case_diff,
    run_wpt_suite,
)


class WptReportTests(unittest.TestCase):
    def test_collect_reports_records_tag_rates(self) -> None:
        with tempfile.TemporaryDirectory() as target_temp, tempfile.TemporaryDirectory() as suite_temp:
            target_dir = Path(target_temp)
            suite_dir = Path(suite_temp)
            (target_dir / f"{REPORT_PREFIX}sample.json").write_text(
                json.dumps(
                    {
                        "cases": [
                            {"id": "a", "expected": "PASS", "actual": "PASS", "category": "pass", "tags": ["dom", "events"]},
                            {
                                "id": "b",
                                "expected": "PASS",
                                "actual": "FAIL",
                                "category": "unexpected-fail",
                                "tags": ["dom"],
                                "failures": ["assertion failed"],
                            },
                            {"id": "c", "expected": "FAIL", "actual": "FAIL", "category": "known-fail", "tags": ["network"]},
                            {"id": "d", "expected": "PASS", "actual": "SKIP", "category": "skip", "tags": []},
                        ]
                    }
                ),
                encoding="utf-8",
            )

            summary, cases = _collect_reports(target_dir, suite_dir)

        self.assertEqual(len(cases), 4)
        self.assertEqual(summary["total"], 4)
        self.assertEqual(summary["pass"], 1)
        self.assertEqual(summary["unexpected_fail"], 1)
        self.assertEqual(summary["known_fail"], 1)
        self.assertEqual(summary["skip"], 1)
        self.assertEqual(summary["pass_rate_percent"], 25.0)
        self.assertEqual(summary["unexpected_fail_rate_percent"], 25.0)
        self.assertEqual(summary["skip_rate_percent"], 25.0)
        self.assertEqual(summary["by_tag"]["dom"]["total"], 2)
        self.assertEqual(summary["by_tag"]["dom"]["pass_rate_percent"], 50.0)
        self.assertEqual(summary["by_tag"]["events"]["pass_rate_percent"], 100.0)

    def test_by_tag_rows_are_stable_and_csv_ready(self) -> None:
        rows = _by_tag_rows(
            {
                "by_tag": {
                    "network": {"total": 2, "pass": 1, "pass_rate_percent": 50.0},
                    "dom": {"total": 1, "pass": 1, "pass_rate_percent": 100.0},
                }
            }
        )

        self.assertEqual([row["tag"] for row in rows], ["dom", "network"])
        self.assertEqual(rows[0]["pass_rate_percent"], 100.0)

    def test_wpt_case_diff_tracks_added_removed_expectation_and_category_changes(self) -> None:
        summary, rows = _wpt_case_diff(
            current_cases=[
                {"id": "kept", "expected": "FAIL", "actual": "PASS", "category": "unexpected-pass", "tags": "dom"},
                {"id": "added", "expected": "PASS", "actual": "PASS", "category": "pass", "tags": "events"},
            ],
            baseline_cases=[
                {"id": "kept", "expected": "PASS", "actual": "FAIL", "category": "unexpected-fail", "tags": "dom"},
                {"id": "removed", "expected": "PASS", "actual": "PASS", "category": "pass", "tags": "network"},
            ],
        )

        self.assertEqual(summary["added"], 1)
        self.assertEqual(summary["removed"], 1)
        self.assertEqual(summary["expectation_changes"], 1)
        self.assertEqual(summary["category_changes"], 1)
        self.assertEqual(summary["total_changes"], 4)
        self.assertEqual([row["kind"] for row in rows], ["added", "removed", "expectation-change", "category-change"])

    def test_wpt_runner_uses_release_nextest_only(self) -> None:
        command = _run_command("nextest")

        self.assertEqual(command[:3], ["cargo", "nextest", "run"])
        self.assertIn("--release", command)
        self.assertNotIn("test", command[:2])

        with self.assertRaisesRegex(RuntimeError, "release nextest runner"):
            _run_command("cargo")

    def test_run_wpt_suite_writes_baseline_diff_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as target_temp, tempfile.TemporaryDirectory() as output_temp, tempfile.TemporaryDirectory() as baseline_temp:
            target_dir = Path(target_temp)
            output_dir = Path(output_temp)
            baseline = Path(baseline_temp) / "baseline.json"
            (target_dir / f"{REPORT_PREFIX}sample.json").write_text(
                json.dumps(
                    {
                        "cases": [
                            {"id": "kept", "expected": "FAIL", "actual": "PASS", "category": "unexpected-pass", "tags": ["dom"]},
                            {"id": "added", "expected": "PASS", "actual": "PASS", "category": "pass", "tags": ["events"]},
                        ]
                    }
                ),
                encoding="utf-8",
            )
            baseline.write_text(
                json.dumps(
                    {
                        "cases": [
                            {"id": "kept", "expected": "PASS", "actual": "FAIL", "category": "unexpected-fail", "tags": ["dom"]},
                            {"id": "removed", "expected": "PASS", "actual": "PASS", "category": "pass", "tags": ["network"]},
                        ]
                    }
                ),
                encoding="utf-8",
            )

            with patch.dict("os.environ", {"CARGO_TARGET_DIR": str(target_dir)}):
                summary = run_wpt_suite(
                    output_dir=output_dir,
                    timeout_seconds=1.0,
                    runner="nextest",
                    compat=None,
                    case_filter=None,
                    tag_filter=None,
                    no_run=True,
                    baseline=baseline,
                )

            self.assertEqual(summary["diff"]["total_changes"], 4)
            self.assertTrue((output_dir / "wpt" / "diff.json").exists())
            self.assertTrue((output_dir / "wpt" / "diff.csv").exists())


if __name__ == "__main__":
    unittest.main()
