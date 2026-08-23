from __future__ import annotations

import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

from moli_benchmark.render_compare import compare_to_baseline, extract_visible_text, run_render_compare_suite


class RenderCompareTests(unittest.TestCase):
    def test_extract_visible_text_skips_script_state(self) -> None:
        html = (
            b"<html><head><title>Example</title></head><body>"
            b"<script>window.state='hidden article text'</script>"
            b"<h1>Visible title</h1><p>Visible body</p>"
            b"</body></html>"
        )
        snapshot = extract_visible_text(html)
        self.assertEqual(snapshot["title"], "Example")
        self.assertIn("Visible title", snapshot["visible_text"])
        self.assertNotIn("hidden article text", snapshot["visible_text"])

    def test_compare_marks_render_match_for_similar_visible_text(self) -> None:
        baseline = (
            b"<html><body><h1>Administrator AI usage data</h1>"
            b"<p>Usage count and active users help administrators understand adoption.</p>"
            b"<p>Teams can compare usage penetration and trends.</p></body></html>"
        )
        target = (
            b"<html><body><h1>Administrator AI usage data</h1>"
            b"<p>Usage count and active users help administrators understand adoption.</p>"
            b"<p>Teams can compare usage penetration and trends.</p></body></html>"
        )
        result = compare_to_baseline(
            baseline_stdout=baseline,
            baseline_category="success-content",
            target_stdout=target,
            target_stderr=b"",
            target_category="success-content",
            min_baseline_text_chars=20,
        )
        self.assertEqual(result["category"], "render-match")
        self.assertTrue(result["ok"])
        self.assertFalse(result["excluded"])
        self.assertGreaterEqual(result["render_quality_score"], 99.0)

    def test_compare_marks_state_only_content_when_raw_has_content_but_dom_does_not(self) -> None:
        baseline = (
            b"<html><body><h1>Administrator AI usage data</h1>"
            b"<p>Usage count and active users help administrators understand adoption.</p>"
            b"<p>Teams can compare usage penetration and trends.</p></body></html>"
        )
        target = (
            b"<html><body><div id=\"root\"></div><script>"
            b"window.__STATE__='Administrator AI usage data Usage count and active users help administrators understand adoption.'"
            b"</script></body></html>"
        )
        result = compare_to_baseline(
            baseline_stdout=baseline,
            baseline_category="success-content",
            target_stdout=target,
            target_stderr=b"",
            target_category="app-shell-only",
            min_baseline_text_chars=20,
        )
        self.assertEqual(result["category"], "state-only-content")
        self.assertFalse(result["ok"])
        self.assertFalse(result["excluded"])
        self.assertLess(result["render_quality_score"], result["raw_content_score"])

    def test_compare_marks_baseline_unusable_when_baseline_failed(self) -> None:
        result = compare_to_baseline(
            baseline_stdout=b"<html><body>captcha</body></html>",
            baseline_category="captcha-or-verification",
            target_stdout=b"<html><body>real content</body></html>",
            target_stderr=b"",
            target_category="success-content",
            min_baseline_text_chars=1,
        )
        self.assertEqual(result["category"], "baseline-unusable")
        self.assertTrue(result["excluded"])

    def test_run_render_compare_excludes_unusable_baseline_from_failures(self) -> None:
        calls: list[tuple[str, str]] = []

        def fake_fetch(
            *,
            target: str,
            info: dict[str, object],
            rank: int,
            domain: str,
            timeout_seconds: float,
            min_body_bytes: int,
            proc_env: dict[str, str],
        ) -> dict[str, object]:
            calls.append((target, domain))
            if target == "chrome":
                stdout = b"<html><body>captcha</body></html>"
                category = "captcha-or-verification"
            else:
                stdout = b"<html><body><h1>Useful article content</h1><p>Real target content body.</p></body></html>"
                category = "success-content"
            return {
                "target": target,
                "rank": rank,
                "domain": domain,
                "url": f"https://{domain}",
                "category": category,
                "ok": category == "success-content",
                "returncode": 0,
                "timed_out": False,
                "elapsed_ms": 1.0,
                "stdout_bytes": len(stdout),
                "stderr_bytes": 0,
                "stdout": stdout,
                "stderr": b"",
                "stderr_tail": "",
                "peak_pss_bytes": 1,
            }

        with TemporaryDirectory() as temp_dir:
            with (
                patch("moli_benchmark.render_compare.resolve_top_sites_source", return_value=("test", Path(temp_dir) / "sites.txt")),
                patch("moli_benchmark.render_compare.load_top_sites_entries", return_value=([(1, "example.test")], ["test"])),
                patch("moli_benchmark.render_compare._execute_fetch", side_effect=fake_fetch),
            ):
                summary = run_render_compare_suite(
                    output_dir=Path(temp_dir),
                    target_matrix={"chrome": {"available": True, "path": "/bin/chrome"}, "moli": {"available": True, "path": "/bin/moli"}},
                    targets=("moli",),
                    baseline_target="chrome",
                    gate_target="moli",
                    limit_override=1,
                )
        self.assertEqual(summary["gate_failures"], 0)
        self.assertEqual(summary["total_failures"], 0)
        self.assertEqual(summary["excluded_rows"], 1)
        self.assertEqual(summary["targets"]["moli"]["evaluated_sites"], 0)
        self.assertEqual(summary["targets"]["moli"]["excluded_sites"], 1)
        self.assertEqual(calls, [("chrome", "example.test")])

    def test_run_render_compare_runs_targets_only_after_baseline_filter(self) -> None:
        calls: list[tuple[str, str]] = []
        article = b"<html><body><h1>Useful article content</h1><p>Real target content body with enough text.</p></body></html>"

        def fake_fetch(
            *,
            target: str,
            info: dict[str, object],
            rank: int,
            domain: str,
            timeout_seconds: float,
            min_body_bytes: int,
            proc_env: dict[str, str],
        ) -> dict[str, object]:
            calls.append((target, domain))
            if target == "chrome" and domain == "skip.example":
                stdout = b""
                category = "timeout"
                timed_out = True
            else:
                stdout = article
                category = "success-content"
                timed_out = False
            return {
                "target": target,
                "rank": rank,
                "domain": domain,
                "url": f"https://{domain}",
                "category": category,
                "ok": category == "success-content",
                "returncode": None if timed_out else 0,
                "timed_out": timed_out,
                "elapsed_ms": 1.0,
                "stdout_bytes": len(stdout),
                "stderr_bytes": 0,
                "stdout": stdout,
                "stderr": b"",
                "stderr_tail": "",
                "peak_pss_bytes": 1,
            }

        with TemporaryDirectory() as temp_dir:
            output_dir = Path(temp_dir)
            with (
                patch("moli_benchmark.render_compare.resolve_top_sites_source", return_value=("test", output_dir / "sites.txt")),
                patch("moli_benchmark.render_compare.load_top_sites_entries", return_value=([(1, "keep.example"), (2, "skip.example")], ["test"])),
                patch("moli_benchmark.render_compare._execute_fetch", side_effect=fake_fetch),
            ):
                summary = run_render_compare_suite(
                    output_dir=output_dir,
                    target_matrix={
                        "chrome": {"available": True, "path": "/bin/chrome"},
                        "moli": {"available": True, "path": "/bin/moli"},
                        "lightpanda": {"available": True, "path": "/bin/lightpanda"},
                    },
                    targets=("moli", "lightpanda"),
                    baseline_target="chrome",
                    gate_target="moli",
                    limit_override=2,
                    min_baseline_text_chars=20,
                    parallelism=1,
                )
            baseline_sites_exists = (output_dir / "render-compare" / "baseline-sites.json").exists()

        self.assertEqual(
            calls,
            [
                ("chrome", "keep.example"),
                ("chrome", "skip.example"),
                ("moli", "keep.example"),
                ("lightpanda", "keep.example"),
            ],
        )
        self.assertTrue(baseline_sites_exists)
        self.assertEqual(summary["site_count"], 2)
        self.assertEqual(summary["evaluated_site_count"], 1)
        self.assertEqual(summary["baseline_excluded_site_count"], 1)
        self.assertEqual(summary["baseline_categories"], {"baseline-usable": 1, "baseline-unusable": 1})
        self.assertEqual(summary["skipped_target_rows"], 2)
        self.assertEqual(summary["targets"]["moli"]["evaluated_sites"], 1)
        self.assertEqual(summary["targets"]["lightpanda"]["evaluated_sites"], 1)

    def test_run_render_compare_requires_gate_target_in_selected_targets(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "gate target"):
            run_render_compare_suite(
                output_dir=Path("/tmp/unused-render-compare-test"),
                target_matrix={},
                targets=("lightpanda",),
                baseline_target="moli",
                gate_target="moli",
                limit_override=1,
            )


if __name__ == "__main__":
    unittest.main()
