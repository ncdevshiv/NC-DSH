from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from moli_benchmark.startup import _startup_formal_gate_rows, run_startup_suite


class StartupTests(unittest.TestCase):
    def test_startup_formal_gate_rows_require_full_workflow(self) -> None:
        rows = _startup_formal_gate_rows(
            profile="formal",
            runs=10,
            include_cdp_first_page=True,
            include_cdp_warm_pages=True,
            cdp_warm_pages=10,
            idle_seconds=(1.0, 5.0, 30.0),
            total_failures=0,
        )

        self.assertTrue(all(row["ok"] for row in rows))

    def test_startup_formal_gate_rows_report_missing_idle_and_failures(self) -> None:
        rows = _startup_formal_gate_rows(
            profile="formal",
            runs=10,
            include_cdp_first_page=True,
            include_cdp_warm_pages=True,
            cdp_warm_pages=10,
            idle_seconds=(1.0, 5.0),
            total_failures=1,
        )
        by_gate = {row["gate"]: row for row in rows}

        self.assertFalse(by_gate["idle-footprint"]["ok"])
        self.assertFalse(by_gate["workload-failures"]["ok"])

    def test_cdp_warm_pages_requires_positive_page_count(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            with self.assertRaisesRegex(RuntimeError, "cdp_warm_pages must be at least 1"):
                run_startup_suite(
                    moli_bin=Path("/does/not/matter"),
                    output_dir=Path(temp_dir),
                    runs=1,
                    timeout_seconds=1.0,
                    include_cdp_warm_pages=True,
                    cdp_warm_pages=0,
                )


if __name__ == "__main__":
    unittest.main()
