from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from moli_benchmark.artifacts import write_json, write_text
from moli_benchmark.publish_readiness import build_publish_readiness


class PublishReadinessTests(unittest.TestCase):
    def test_smoke_report_is_investigation_with_known_invalid_items(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            output_dir = Path(temp_dir)
            write_json(output_dir / "environment.json", {})
            write_json(output_dir / "versions.json", {})
            write_json(output_dir / "summary.json", {})
            write_text(output_dir / "summary.md", "# summary\n")
            write_json(output_dir / "publish-readiness.json", {})
            write_json(output_dir / "report-data.json", {})
            write_text(output_dir / "index.html", "<!doctype html>\n")

            readiness = build_publish_readiness(
                output_dir=output_dir,
                versions={
                    "targets": {
                        "moli": {"available": True},
                        "lightpanda": {"available": False},
                        "chrome": {"available": True},
                        "obscura": {"available": True},
                    }
                },
                summaries=[
                    {
                        "suite": "amiibo-crawler",
                        "profile": "smoke",
                        "gate_failures": 0,
                        "targets": {"moli": {}},
                    }
                ],
            )

        self.assertEqual(readiness["status"], "investigation")
        failed_names = {item["name"] for item in readiness["known_invalid_items"]}
        self.assertIn("target matrix", failed_names)
        self.assertIn("synthetic formal matrix", failed_names)
        self.assertIn("amiibo formal crawler", failed_names)

    def test_full_zero_failure_report_is_publishable(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            output_dir = Path(temp_dir)
            write_json(output_dir / "environment.json", {})
            write_json(output_dir / "versions.json", {})
            write_json(output_dir / "summary.json", {})
            write_text(output_dir / "summary.md", "# summary\n")
            write_json(output_dir / "publish-readiness.json", {})
            write_json(output_dir / "report-data.json", {})
            write_text(output_dir / "index.html", "<!doctype html>\n")

            readiness = build_publish_readiness(
                output_dir=output_dir,
                versions={
                    "targets": {
                        "moli": {"available": True},
                        "lightpanda": {"available": True},
                        "chrome": {"available": True},
                        "obscura": {"available": True},
                    }
                },
                summaries=[
                    {
                        "suite": "synthetic-matrix",
                        "profile": "formal",
                        "gate_failures": 0,
                        "formal_gate_rows": [
                            {"gate": "profile", "ok": True},
                            {"gate": "runs", "ok": True},
                            {"gate": "repeats", "ok": True},
                            {"gate": "concurrency_levels", "ok": True},
                            {"gate": "cases", "ok": True},
                            {"gate": "workload_failures", "ok": True},
                            {"gate": "stability_failures", "ok": True},
                        ],
                        "targets": {"moli": {}, "lightpanda": {}, "chrome": {}, "obscura": {}},
                    },
                    {
                        "suite": "startup",
                        "profile": "formal",
                        "total_failures": 0,
                        "gate_failures": 0,
                        "formal_gate_rows": [
                            {"gate": "profile", "ok": True},
                            {"gate": "runs", "ok": True},
                            {"gate": "cdp-first-page", "ok": True},
                            {"gate": "cdp-warm-pages", "ok": True},
                            {"gate": "idle-footprint", "ok": True},
                            {"gate": "workload-failures", "ok": True},
                        ],
                    },
                    {
                        "suite": "wpt",
                        "cases": 10,
                        "total_failures": 0,
                        "summary": {"unexpected_fail": 0, "skip": 0, "known_fail": 0},
                    },
                    {
                        "suite": "cdp-smoke",
                        "profile": "formal",
                        "total_failures": 0,
                        "gate_failures": 0,
                        "client_coverage": {"raw_cdp": True, "playwright": True, "puppeteer": True},
                        "client_rows": [
                            {"client": "raw_cdp", "gate_ok": True},
                            {"client": "playwright", "gate_ok": True},
                            {"client": "puppeteer", "gate_ok": True},
                        ],
                    },
                    {"suite": "amiibo-crawler", "profile": "formal", "gate_failures": 0},
                    {
                        "suite": "wild-web",
                        "gate_failures": 0,
                        "seeds": ["zhihu-home", "toutiao-home"],
                        "targets": {"moli": {"extraction_failures": 0}},
                    },
                ],
            )

        self.assertEqual(readiness["status"], "publishable")
        self.assertEqual(readiness["known_invalid_items"], [])

    def test_horizontal_matrix_accepts_engine_driver_target_variants(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            output_dir = Path(temp_dir)
            write_json(output_dir / "environment.json", {})
            write_json(output_dir / "versions.json", {})
            write_json(output_dir / "summary.json", {})
            write_text(output_dir / "summary.md", "# summary\n")
            write_json(output_dir / "publish-readiness.json", {})
            write_json(output_dir / "report-data.json", {})
            write_text(output_dir / "index.html", "<!doctype html>\n")

            readiness = build_publish_readiness(
                output_dir=output_dir,
                versions={
                    "targets": {
                        "moli": {"available": True},
                        "lightpanda": {"available": True},
                        "chrome": {"available": True},
                        "obscura": {"available": True},
                    }
                },
                summaries=[
                    {
                        "suite": "cdp-session",
                        "targets": {
                            "moli-cdp": {"engine": "moli", "driver": "cdp"},
                            "lightpanda-cdp": {"engine": "lightpanda", "driver": "cdp"},
                            "chrome-cdp": {"engine": "chrome", "driver": "cdp"},
                            "obscura-cdp": {"engine": "obscura", "driver": "cdp"},
                        },
                    }
                ],
            )

        horizontal = next(check for check in readiness["checks"] if check["name"] == "horizontal comparison")
        self.assertTrue(horizontal["ok"])


if __name__ == "__main__":
    unittest.main()
