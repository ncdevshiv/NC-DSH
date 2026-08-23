from __future__ import annotations

import asyncio
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from moli_benchmark.amiibo_crawler import (
    AMIIBO_CONCURRENCY_MATRIX,
    AMIIBO_MODES,
    PageSession,
    _classify_amiibo_error,
    _collect_page_assertion_failures,
    _crawl_with_endpoint,
    _run_process_mode,
    _row_failure_kind,
    _summarize_rows,
    _summarize_serve_resources,
    run_amiibo_crawler_suite,
)


class AmiiboCrawlerTests(unittest.TestCase):
    class _FakeWebSocket:
        def __init__(self) -> None:
            self.close_count = 0

        async def close(self) -> None:
            self.close_count += 1

    class _FakeClient:
        def __init__(self, websocket: "AmiiboCrawlerTests._FakeWebSocket") -> None:
            self.websocket = websocket

    def test_crawl_returns_when_all_page_sessions_fail(self) -> None:
        async def fail_create_session(endpoint: str, timeout_seconds: float) -> object:
            raise RuntimeError("session failed")

        async def run() -> dict[str, object]:
            with patch("moli_benchmark.amiibo_crawler._create_page_session", fail_create_session):
                return await asyncio.wait_for(
                    _crawl_with_endpoint(
                        endpoint="http://127.0.0.1:1",
                        pool=2,
                        limit=1,
                        url="https://demo-browser.lightpanda.io/amiibo/",
                        timeout_seconds=0.1,
                    ),
                    timeout=1.0,
                )

        result = asyncio.run(run())

        self.assertEqual(result["known_count"], 1)
        self.assertEqual(result["expected_pages"], 1)
        errors = result["errors"]
        self.assertIsInstance(errors, list)
        self.assertEqual(len(errors), 2)
        self.assertEqual({error["stage"] for error in errors}, {"create-session"})

    def test_crawl_closes_created_sessions_when_creation_is_cancelled(self) -> None:
        websocket = self._FakeWebSocket()
        calls = 0

        async def create_session(endpoint: str, timeout_seconds: float) -> PageSession:
            nonlocal calls
            calls += 1
            if calls == 1:
                return PageSession(client=self._FakeClient(websocket), session_id="first")
            await asyncio.sleep(10)
            raise AssertionError("unreachable")

        async def run() -> None:
            with patch("moli_benchmark.amiibo_crawler._create_page_session", create_session):
                await asyncio.wait_for(
                    _crawl_with_endpoint(
                        endpoint="http://127.0.0.1:1",
                        pool=2,
                        limit=1,
                        url="https://demo-browser.lightpanda.io/amiibo/",
                        timeout_seconds=30.0,
                    ),
                    timeout=0.01,
                )

        with self.assertRaises(TimeoutError):
            asyncio.run(run())

        self.assertGreaterEqual(websocket.close_count, 1)

    def test_process_mode_closes_created_sessions_when_creation_times_out(self) -> None:
        websocket = self._FakeWebSocket()
        calls = 0

        class Serve:
            def __init__(self, worker_id: int) -> None:
                self.endpoint = f"http://127.0.0.1:{worker_id}"

        def start_serve(target: str, binary: Path, timeout_seconds: float) -> Serve:
            return Serve(calls + 1)

        def stop_serve(serve: Serve) -> dict[str, object]:
            return {"endpoint": serve.endpoint, "resources": {"peak_pss_bytes": 0, "peak_cpu_percent": 0.0}}

        async def create_session(endpoint: str, timeout_seconds: float) -> PageSession:
            nonlocal calls
            calls += 1
            if calls == 1:
                return PageSession(client=self._FakeClient(websocket), session_id="first")
            await asyncio.sleep(10)
            raise AssertionError("unreachable")

        with (
            patch("moli_benchmark.amiibo_crawler.start_target_serve", start_serve),
            patch("moli_benchmark.amiibo_crawler.stop_target_serve", stop_serve),
            patch("moli_benchmark.amiibo_crawler._create_page_session", create_session),
        ):
            row, detail = _run_process_mode(
                target="moli",
                binary=Path("/tmp/moli"),
                pool=2,
                limit=1,
                url="https://demo-browser.lightpanda.io/amiibo/",
                timeout_seconds=0.01,
            )

        self.assertFalse(row["ok"])
        self.assertEqual(row["failure_kind"], "timeout")
        self.assertEqual(len(detail["serve"]["workers"]), 2)
        self.assertGreaterEqual(websocket.close_count, 1)

    def test_page_assertions_require_extracted_fields(self) -> None:
        failures = _collect_page_assertion_failures(
            [
                {
                    "url": "https://demo-browser.lightpanda.io/amiibo/",
                    "ready_state": "complete",
                    "title": "Sandy",
                    "text_length": 222,
                    "link_count": 12,
                    "fields": {
                        "name": "Sandy",
                        "game": "Animal Crossing",
                        "serie": "Animal Crossing",
                        "imageSrc": "https://example.test/sandy.png",
                        "altCount": 10,
                    },
                },
                {"url": "", "title": "", "text_length": 0, "link_count": -1, "worker": 3},
                {
                    "url": "https://demo-browser.lightpanda.io/amiibo/stale.html",
                    "ready_state": "complete",
                    "title": "Amiibo Character",
                    "text_length": 80,
                    "link_count": 0,
                    "worker": 4,
                    "fields": {
                        "name": "Amiibo Character",
                        "game": "Amiibo Game",
                        "serie": "Amiibo Serie",
                        "imageSrc": "",
                        "altCount": 0,
                    },
                },
            ]
        )

        self.assertEqual(
            failures,
            [
                {
                    "url": "",
                    "worker": 3,
                    "failures": ["missing-url", "missing-title", "document-not-complete", "missing-body-text", "missing-link-count", "missing-fields"],
                },
                {
                    "url": "https://demo-browser.lightpanda.io/amiibo/stale.html",
                    "worker": 4,
                    "failures": [
                        "missing-amiibo-name",
                        "missing-amiibo-series",
                        "missing-game-series",
                        "missing-image-src",
                    ],
                },
            ],
        )

    def test_failure_classification_prefers_specific_causes(self) -> None:
        self.assertEqual(_classify_amiibo_error("crawler run exceeded 30.0s wall timeout"), "timeout")
        self.assertEqual(_classify_amiibo_error("CDP command id=1 failed"), "protocol-error")
        self.assertEqual(_classify_amiibo_error("Page.navigate failed"), "navigation-error")
        self.assertEqual(_classify_amiibo_error("Runtime.evaluate returned an unexpected result"), "script-error")
        self.assertEqual(
            _row_failure_kind(
                ok=False,
                pages=49,
                expected_pages=50,
                crawler_errors=0,
                assertion_failures=0,
            ),
            "page-count-mismatch",
        )
        self.assertEqual(
            _row_failure_kind(
                ok=False,
                pages=50,
                expected_pages=50,
                crawler_errors=0,
                assertion_failures=1,
            ),
            "assertion-failure",
        )
        self.assertEqual(
            _row_failure_kind(
                ok=False,
                pages=None,
                expected_pages=50,
                crawler_errors=0,
                assertion_failures=0,
            ),
            "error",
        )

    def test_process_resource_summary_aggregates_worker_processes(self) -> None:
        summary = _summarize_serve_resources(
            [
                {"resources": {"peak_pss_bytes": 10, "peak_cpu_percent": 1.5, "peak_process_count": 1}},
                {"resources": {"peak_pss_bytes": 20, "peak_cpu_percent": 2.5, "peak_process_count": 1}},
            ]
        )

        self.assertEqual(summary["peak_pss_bytes"], 30)
        self.assertEqual(summary["peak_cpu_percent"], 4.0)
        self.assertEqual(summary["peak_process_count"], 2)
        self.assertEqual(summary["max_worker_peak_pss_bytes"], 20)
        self.assertEqual(summary["worker_count"], 2)

    def test_row_summary_keeps_failure_kind_counts(self) -> None:
        summary = _summarize_rows(
            [
                {"ok": True, "elapsed_ms": 10.0, "browser_peak_pss_bytes": 100, "assertion_failures": 0},
                {
                    "ok": False,
                    "elapsed_ms": 20.0,
                    "browser_peak_pss_bytes": 200,
                    "assertion_failures": 2,
                    "failure_kind": "assertion-failure",
                },
            ]
        )

        self.assertEqual(summary["runs"], 2)
        self.assertEqual(summary["passes"], 1)
        self.assertEqual(summary["failures"], 1)
        self.assertEqual(summary["assertion_failures"], 2)
        self.assertEqual(summary["failure_kinds"], {"assertion-failure": 1})
        self.assertEqual(summary["elapsed_ms"]["p50"], 10.0)

    def test_formal_profile_requirements_are_reported_without_browser(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            summary = run_amiibo_crawler_suite(
                output_dir=Path(temp_dir),
                target_matrix={"moli": {"available": False}},
                profile="formal",
                targets=("moli",),
                pools=AMIIBO_CONCURRENCY_MATRIX,
                modes=AMIIBO_MODES,
                runs=1,
                limit=0,
                timeout_seconds=0.1,
                gate_target="moli",
            )

        self.assertEqual(summary["profile"], "formal")
        self.assertEqual(summary["profile_failures"], 0)
        self.assertEqual(summary["formal_requirements"]["limit"]["ok"], True)
        self.assertEqual(summary["formal_requirements"]["pool"]["ok"], True)
        self.assertEqual(summary["formal_requirements"]["modes"]["ok"], True)
        self.assertEqual(summary["targets"]["moli-cdp"]["failure_kinds"], {"target-unavailable": 12})
        self.assertEqual(summary["targets"]["moli-cdp"]["engine"], "moli")
        self.assertEqual(summary["targets"]["moli-cdp"]["driver"], "cdp")

    def test_smoke_profile_does_not_gate_on_formal_requirements(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            summary = run_amiibo_crawler_suite(
                output_dir=Path(temp_dir),
                target_matrix={"moli": {"available": False}},
                profile="smoke",
                targets=("moli",),
                pools=(1,),
                modes=("session",),
                runs=1,
                limit=5,
                timeout_seconds=0.1,
                gate_target="chrome",
            )

        self.assertEqual(summary["profile"], "smoke")
        self.assertEqual(summary["profile_failures"], 0)
        self.assertEqual(summary["formal_requirements"]["limit"]["ok"], False)
        self.assertEqual(summary["formal_requirements"]["pool"]["ok"], False)
        self.assertEqual(summary["formal_requirements"]["modes"]["ok"], False)
        self.assertEqual(summary["gate_failures"], 0)


if __name__ == "__main__":
    unittest.main()
