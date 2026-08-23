from __future__ import annotations

import json
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

from moli_benchmark.wpt_cross.coverage import (
    audit_wasm_focused_coverage,
    main,
)


class WptCrossCoverageTests(unittest.TestCase):
    def test_wasm_focused_coverage_ignores_resources_but_flags_missing_top_level(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            wpt_root = Path(temp_dir)
            wasm_root = wpt_root / "wasm"
            wasm_root.mkdir()
            (wasm_root / "case.html").write_text(
                """<script src="/resources/testharness.js"></script>
<script src="/resources/testharnessreport.js"></script>
<script>test(() => {}, "ok");</script>
""",
                encoding="utf-8",
            )
            (wasm_root / "feature.any.js").write_text(
                """// META: global=window,worker
promise_test(async () => {}, "any");
""",
                encoding="utf-8",
            )
            (wasm_root / "unsupported.any.js").write_text(
                """// META: global=window,worker
promise_test(async () => fetch("/unimplemented.py"), "unsupported");
""",
                encoding="utf-8",
            )
            resources = wasm_root / "resources"
            resources.mkdir()
            (resources / "helper.html").write_text(
                """<script src="/resources/testharness.js"></script>""",
                encoding="utf-8",
            )

            audit = audit_wasm_focused_coverage(wpt_root)

        self.assertFalse(audit["ok"])
        self.assertEqual(audit["source_case_count"], 4)
        self.assertEqual(audit["top_level_case_count"], 3)
        self.assertEqual(
            audit["ignored_support_resources"],
            ["wasm/resources/helper.html"],
        )
        self.assertEqual(
            audit["missing_top_level_cases"],
            ["wasm/unsupported.any.js"],
        )
        self.assertEqual(audit["enumerated_support_resource_paths"], [])
        self.assertEqual(audit["duplicate_enumerated_case_paths"], [])
        self.assertEqual(audit["missing_known_failure_rules"], [])
        self.assertEqual(audit["extra_enumerated_base_paths"], [])

    def test_wasm_focused_coverage_flags_enumerated_support_resources(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            wpt_root = Path(temp_dir)
            wasm_root = wpt_root / "wasm"
            resources = wasm_root / "resources"
            resources.mkdir(parents=True)
            (wasm_root / "case.html").write_text(
                """<script src="/resources/testharness.js"></script>
<script src="/resources/testharnessreport.js"></script>
<script>test(() => {}, "ok");</script>
""",
                encoding="utf-8",
            )
            (resources / "helper.html").write_text(
                """<script src="/resources/testharness.js"></script>
<script>test(() => {}, "helper");</script>
""",
                encoding="utf-8",
            )

            with patch(
                "moli_benchmark.wpt_cross.coverage.enumerate_cases",
                return_value=[
                    SimpleNamespace(case_path="wasm/case.html"),
                    SimpleNamespace(case_path="wasm/resources/helper.html"),
                ],
            ):
                audit = audit_wasm_focused_coverage(wpt_root)

        self.assertFalse(audit["ok"])
        self.assertEqual(audit["missing_top_level_cases"], [])
        self.assertEqual(
            audit["enumerated_support_resource_paths"],
            ["wasm/resources/helper.html"],
        )
        self.assertEqual(audit["extra_enumerated_base_paths"], [])

    def test_wasm_focused_coverage_flags_duplicate_enumerated_cases(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            wpt_root = Path(temp_dir)
            wasm_root = wpt_root / "wasm"
            wasm_root.mkdir()
            (wasm_root / "case.html").write_text(
                """<script src="/resources/testharness.js"></script>
<script src="/resources/testharnessreport.js"></script>
<script>test(() => {}, "ok");</script>
""",
                encoding="utf-8",
            )

            with patch(
                "moli_benchmark.wpt_cross.coverage.enumerate_cases",
                return_value=[
                    SimpleNamespace(case_path="wasm/case.html"),
                    SimpleNamespace(case_path="wasm/case.html"),
                ],
            ):
                audit = audit_wasm_focused_coverage(wpt_root)

        self.assertFalse(audit["ok"])
        self.assertEqual(audit["missing_top_level_cases"], [])
        self.assertEqual(
            audit["duplicate_enumerated_case_paths"],
            ["wasm/case.html"],
        )
        self.assertEqual(audit["extra_enumerated_base_paths"], [])

    def test_wasm_focused_coverage_flags_known_failure_rules_outside_case_set(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            wpt_root = root / "wpt"
            wasm_root = wpt_root / "wasm"
            wasm_root.mkdir(parents=True)
            (wasm_root / "case.html").write_text(
                """<script src="/resources/testharness.js"></script>
<script src="/resources/testharnessreport.js"></script>
<script>test(() => {}, "ok");</script>
""",
                encoding="utf-8",
            )
            manifest = root / "known.json"
            manifest.write_text(
                json.dumps(
                    {
                        "rules": [
                            {
                                "case_path": "wasm/case.html",
                                "category": "known",
                                "expected_status": "fail",
                            },
                            {
                                "case_path": "wasm/not-enumerated.html",
                                "category": "stale",
                                "expected_status": "fail",
                            },
                        ]
                    }
                ),
                encoding="utf-8",
            )

            audit = audit_wasm_focused_coverage(
                wpt_root,
                known_failures=manifest,
            )

        self.assertFalse(audit["ok"])
        self.assertEqual(audit["known_failure_rule_count"], 2)
        self.assertEqual(
            audit["known_failure_category_counts"],
            {"known": 1, "stale": 1},
        )
        self.assertEqual(audit["missing_known_failure_category_counts"], {"stale": 1})
        self.assertEqual(
            audit["missing_known_failure_rules"],
            ["wasm/not-enumerated.html"],
        )
        self.assertEqual(
            audit["missing_known_failure_rule_details"],
            [
                {
                    "case_path": "wasm/not-enumerated.html",
                    "category": "stale",
                }
            ],
        )

    def test_wasm_focused_coverage_reports_uncategorized_known_failure_rules(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            wpt_root = root / "wpt"
            wasm_root = wpt_root / "wasm"
            wasm_root.mkdir(parents=True)
            (wasm_root / "case.html").write_text(
                """<script src="/resources/testharness.js"></script>
<script src="/resources/testharnessreport.js"></script>
<script>test(() => {}, "ok");</script>
""",
                encoding="utf-8",
            )
            manifest = root / "known.json"
            manifest.write_text(
                json.dumps(
                    {
                        "rules": [
                            {
                                "case_path": "wasm/not-enumerated.html",
                                "expected_status": "fail",
                            },
                        ]
                    }
                ),
                encoding="utf-8",
            )

            audit = audit_wasm_focused_coverage(
                wpt_root,
                known_failures=manifest,
            )

        self.assertFalse(audit["ok"])
        self.assertEqual(
            audit["known_failure_category_counts"],
            {"uncategorized": 1},
        )
        self.assertEqual(
            audit["missing_known_failure_rule_details"],
            [
                {
                    "case_path": "wasm/not-enumerated.html",
                    "category": "uncategorized",
                }
            ],
        )

    def test_wasm_focused_coverage_flags_extra_enumerated_base_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            wpt_root = Path(temp_dir)
            wasm_root = wpt_root / "wasm"
            wasm_root.mkdir()
            (wasm_root / "case.html").write_text(
                """<script src="/resources/testharness.js"></script>
<script src="/resources/testharnessreport.js"></script>
<script>test(() => {}, "ok");</script>
""",
                encoding="utf-8",
            )

            with patch(
                "moli_benchmark.wpt_cross.coverage.enumerate_cases",
                return_value=[
                    SimpleNamespace(case_path="wasm/case.html"),
                    SimpleNamespace(case_path="wasm/generated.html?variant=1"),
                ],
            ):
                audit = audit_wasm_focused_coverage(wpt_root)

        self.assertFalse(audit["ok"])
        self.assertEqual(audit["missing_top_level_cases"], [])
        self.assertEqual(
            audit["extra_enumerated_base_paths"],
            ["wasm/generated.html"],
        )

    def test_wasm_focused_coverage_cli_writes_json_and_returns_nonzero_for_gap(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            wpt_root = Path(temp_dir) / "wpt"
            output = Path(temp_dir) / "coverage.json"
            manifest = Path(temp_dir) / "known.json"
            wasm_root = wpt_root / "wasm"
            wasm_root.mkdir(parents=True)
            (wasm_root / "unsupported.any.js").write_text(
                """promise_test(async () => fetch("/unimplemented.py"), "unsupported");""",
                encoding="utf-8",
            )
            manifest.write_text(json.dumps({"rules": []}), encoding="utf-8")

            with redirect_stdout(StringIO()):
                exit_code = main(
                    [
                        "--wpt-root",
                        str(wpt_root),
                        "--known-failures",
                        str(manifest),
                        "--json-output",
                        str(output),
                    ]
                )
            audit = json.loads(output.read_text(encoding="utf-8"))

        self.assertEqual(exit_code, 1)
        self.assertFalse(audit["ok"])
        self.assertEqual(audit["missing_top_level_cases"], ["wasm/unsupported.any.js"])


if __name__ == "__main__":
    unittest.main()
