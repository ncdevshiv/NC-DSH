from __future__ import annotations

import importlib.util
import sys
import types
import unittest
import urllib.parse
from pathlib import Path


SCRIPT_PATH = (
    Path(__file__).resolve().parents[1] / "scripts" / "probe-cdp-memory-ablation.py"
)


def load_probe_module():
    sys.modules.setdefault("websockets", types.ModuleType("websockets"))
    sys.modules.setdefault("websockets.asyncio", types.ModuleType("websockets.asyncio"))
    websockets_client = types.ModuleType("websockets.asyncio.client")
    websockets_client.ClientConnection = object
    sys.modules.setdefault("websockets.asyncio.client", websockets_client)
    spec = importlib.util.spec_from_file_location("probe_cdp_memory_ablation", SCRIPT_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {SCRIPT_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class CdpMemoryAblationProbeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.probe = load_probe_module()

    def test_fixed_probe_data_page_url_has_no_shared_worker_constructor(self) -> None:
        url = self.probe.fixed_probe_page_url(
            self.probe.FIXED_PROBE_DATA_PAGES,
            index=2,
            payload_kib=1,
        )

        html = urllib.parse.unquote(url.removeprefix("data:text/html,"))
        self.assertIn('data-fixed-probe="data-pages"', html)
        self.assertIn('data-index="2"', html)
        self.assertNotIn("new SharedWorker", html)

    def test_fixed_probe_shared_worker_key_modes_are_explicit(self) -> None:
        same_key = urllib.parse.unquote(
            self.probe.fixed_probe_page_url(
                self.probe.FIXED_PROBE_SHARED_WORKER_SAME_KEY,
                index=1,
                payload_kib=0,
            ).removeprefix("data:text/html,")
        )
        distinct_key = urllib.parse.unquote(
            self.probe.fixed_probe_page_url(
                self.probe.FIXED_PROBE_SHARED_WORKER_DISTINCT_KEY,
                index=3,
                payload_kib=0,
            ).removeprefix("data:text/html,")
        )

        self.assertIn('"fixed-shared-worker"', same_key)
        self.assertIn('"fixed-shared-worker-3"', distinct_key)
        self.assertIn("new SharedWorker", same_key)
        self.assertIn("new SharedWorker", distinct_key)

    def test_fixed_probe_shared_worker_http_fixture_uses_same_origin_worker_url(self) -> None:
        url = self.probe.fixed_probe_page_url(
            self.probe.FIXED_PROBE_SHARED_WORKER_SAME_KEY,
            index=2,
            payload_kib=0,
            base_url="http://127.0.0.1:43210",
        )
        parsed = urllib.parse.urlparse(url)
        query = urllib.parse.parse_qs(parsed.query)

        self.assertEqual(parsed.scheme, "http")
        self.assertEqual(parsed.netloc, "127.0.0.1:43210")
        self.assertEqual(parsed.path, "/fixed-page")
        self.assertEqual(query["probe"], ["shared-worker-same-key"])
        self.assertEqual(query["index"], ["2"])

        html = self.probe.fixed_probe_page_html(
            self.probe.FIXED_PROBE_SHARED_WORKER_SAME_KEY,
            index=2,
            payload_kib=0,
            worker_script_url="/fixed-shared-worker.js",
        )

        self.assertIn('new SharedWorker(url, "fixed-shared-worker")', html)
        self.assertIn('const url = "/fixed-shared-worker.js";', html)

    def test_fixed_probe_popup_and_background_modes_do_not_install_shared_worker(self) -> None:
        self.assertIn(
            self.probe.FIXED_PROBE_BACKGROUND_TARGETS,
            self.probe.FIXED_PROBE_CHOICES,
        )
        self.assertIn(
            self.probe.FIXED_PROBE_POPUP_TARGETS,
            self.probe.FIXED_PROBE_CHOICES,
        )
        popup = urllib.parse.unquote(
            self.probe.fixed_probe_page_url(
                self.probe.FIXED_PROBE_POPUP_TARGETS,
                index=4,
                payload_kib=0,
            ).removeprefix("data:text/html,")
        )
        background = urllib.parse.unquote(
            self.probe.fixed_probe_page_url(
                self.probe.FIXED_PROBE_BACKGROUND_TARGETS,
                index=5,
                payload_kib=0,
            ).removeprefix("data:text/html,")
        )

        self.assertIn('data-fixed-probe="popup-targets"', popup)
        self.assertIn('data-index="4"', popup)
        self.assertNotIn("new SharedWorker", popup)
        self.assertIn('data-fixed-probe="background-targets"', background)
        self.assertIn('data-index="5"', background)
        self.assertNotIn("new SharedWorker", background)

    def test_fixed_probe_dedicated_worker_mode_is_explicit(self) -> None:
        self.assertIn(
            self.probe.FIXED_PROBE_DEDICATED_WORKER,
            self.probe.FIXED_PROBE_CHOICES,
        )
        self.assertIn(
            self.probe.FIXED_PROBE_DEDICATED_WORKER,
            self.probe.FIXED_PROBE_WORKER_MODES,
        )

        html = urllib.parse.unquote(
            self.probe.fixed_probe_page_url(
                self.probe.FIXED_PROBE_DEDICATED_WORKER,
                index=6,
                payload_kib=0,
            ).removeprefix("data:text/html,")
        )

        self.assertIn('data-fixed-probe="dedicated-worker"', html)
        self.assertIn("new Worker", html)
        self.assertIn("setInterval", html)
        self.assertNotIn("new SharedWorker", html)

    def test_fixed_probe_different_browser_context_mode_has_plain_page_payload(self) -> None:
        self.assertIn(
            self.probe.FIXED_PROBE_DIFFERENT_BROWSER_CONTEXTS,
            self.probe.FIXED_PROBE_CHOICES,
        )

        html = urllib.parse.unquote(
            self.probe.fixed_probe_page_url(
                self.probe.FIXED_PROBE_DIFFERENT_BROWSER_CONTEXTS,
                index=7,
                payload_kib=0,
            ).removeprefix("data:text/html,")
        )

        self.assertIn('data-fixed-probe="different-browser-contexts"', html)
        self.assertIn('data-index="7"', html)
        self.assertNotIn("new Worker", html)
        self.assertNotIn("new SharedWorker", html)

    def test_compact_snapshot_includes_per_target_heap(self) -> None:
        snapshot = {
            "label": "fixed_before_close_targets",
            "resources": {"rss_bytes": 1024 * 1024, "pss_bytes": 512 * 1024, "thread_count": 7},
            "smaps": {
                "categories": {},
                "top_mappings": [],
                "anonymous_histogram_by_vma_size": {},
            },
            "extra": {
                "perTargetHeap": [
                    {
                        "index": 1,
                        "targetId": "TID-1",
                        "sessionId": "SID-1",
                        "browserContextId": "BID-1",
                        "heap": {
                            "elapsed_ms": 1.0,
                            "seen_count": 2,
                            "response": {
                                "usedSize": 2 * 1024 * 1024,
                                "totalSize": 4 * 1024 * 1024,
                                "totalPhysicalSize": 3 * 1024 * 1024,
                                "mallocedMemory": 0,
                                "externalMemory": 0,
                                "numberOfNativeContexts": 1,
                                "numberOfDetachedContexts": 0,
                            },
                        },
                    }
                ]
            },
        }

        compact = self.probe.compact_snapshot(snapshot)

        self.assertEqual(compact["per_target_heap"][0]["index"], 1)
        self.assertEqual(compact["per_target_heap"][0]["targetId"], "TID-1")
        self.assertEqual(compact["per_target_heap"][0]["browserContextId"], "BID-1")
        self.assertEqual(compact["per_target_heap"][0]["heap"]["used_mib"], 2.0)
        self.assertEqual(compact["per_target_heap"][0]["heap"]["native_contexts"], 1)


if __name__ == "__main__":
    unittest.main()
