from __future__ import annotations

import http.client
import json
import tempfile
import unittest
import urllib.error
import urllib.request
from pathlib import Path
from unittest import mock

from moli_benchmark.agent_episode import (
    AgentEpisodeError,
    AgentEpisodeFixtureServer,
    DEFAULT_MANIFEST_PATH,
    _aggregate_resource_samples,
    _known_error_counts,
    _navigate,
    _step_assertion_count,
    load_agent_episode_manifest,
    run_agent_episode_suite,
)
from moli_benchmark.agent_episode_fixture import response_for_agent_path
from moli_benchmark.agent_episode_report import write_agent_episode_report


class AgentEpisodeManifestTests(unittest.TestCase):
    def test_checked_in_manifest_has_required_contract_cases(self) -> None:
        manifest = load_agent_episode_manifest()
        episode_ids = {str(episode["id"]) for episode in manifest.episodes}

        self.assertEqual(manifest.fixture_version, "agent-episode-fixture-v1")
        self.assertEqual(len(manifest.sha256), 64)
        self.assertTrue(
            {
                "observe-static",
                "fill-reactive-form",
                "click-same-document",
                "click-cross-document",
                "failed-navigation",
                "dynamic-controls",
                "idle-resume",
                "episode-isolation",
            }.issubset(episode_ids)
        )

    def test_manifest_rejects_duplicate_episode_ids(self) -> None:
        payload = json.loads(DEFAULT_MANIFEST_PATH.read_text(encoding="utf-8"))
        payload["episodes"].append(payload["episodes"][0])
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "manifest.json"
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(AgentEpisodeError, "duplicate episode id"):
                load_agent_episode_manifest(path)

    def test_manifest_rejects_invalid_observation_contract(self) -> None:
        payload = json.loads(DEFAULT_MANIFEST_PATH.read_text(encoding="utf-8"))
        payload["episodes"][0]["steps"][0]["expect"]["text_contains"] = "not-an-array"
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "manifest.json"
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaisesRegex(AgentEpisodeError, "text_contains"):
                load_agent_episode_manifest(path)


class AgentEpisodeFixtureTests(unittest.TestCase):
    def test_fixture_serves_reactive_page_and_records_request(self) -> None:
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with AgentEpisodeFixtureServer() as fixture:
            with opener.open(fixture.url("/agent/fill-reactive-form"), timeout=2) as response:
                body = response.read().decode("utf-8")
            requests = fixture.requests

        self.assertIn("Reactive form ready", body)
        self.assertIn("queueMicrotask", body)
        self.assertEqual(requests[-1]["path"], "/agent/fill-reactive-form")

    def test_isolation_fixture_escapes_inline_script_end_tags(self) -> None:
        token = "</ScRiPt><script>globalThis.injected=true</script>"
        response = response_for_agent_path("/agent/isolation", {"token": [token]})

        self.assertIsNotNone(response)
        body = response.decode("utf-8") if response is not None else ""
        self.assertNotIn("</ScRiPt><script>", body)
        self.assertIn(
            r'globalThis.__agentIsolationToken = "<\/ScRiPt><script>globalThis.injected=true<\/script>";',
            body,
        )

    def test_failure_fixture_resets_connection_before_response(self) -> None:
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with AgentEpisodeFixtureServer() as fixture:
            with self.assertRaises(
                (ConnectionResetError, http.client.RemoteDisconnected, urllib.error.URLError)
            ):
                opener.open(fixture.url("/agent/reset-before-response"), timeout=2).read()


class AgentEpisodeNavigationTests(unittest.IsolatedAsyncioTestCase):
    async def test_document_events_share_timeout_budget(self) -> None:
        class Clock:
            def __init__(self) -> None:
                self._times = iter((100.0, 100.2, 100.9))

            def time(self) -> float:
                return next(self._times)

        cases = (
            (None, {}),
            (
                "net::ERR_CONNECTION_RESET",
                {
                    "expect_error_text_contains": "ERR_CONNECTION_RESET",
                    "expect_error_document": True,
                },
            ),
        )
        for error_text, expectation in cases:
            with self.subTest(error_text=error_text):
                client = mock.Mock(current_sequence=7)
                client.command = mock.AsyncMock(
                    return_value=mock.Mock(
                        response={"result": {"errorText": error_text}}
                    )
                )
                client.wait_for_event = mock.AsyncMock()
                with mock.patch(
                    "moli_benchmark.agent_episode.asyncio.get_running_loop",
                    return_value=Clock(),
                ):
                    await _navigate(
                        client,  # type: ignore[arg-type]
                        "session-1",
                        "http://fixture.test/next",
                        1.0,
                        **expectation,
                    )

                waits = client.wait_for_event.await_args_list
                self.assertEqual(
                    [call.args[0] for call in waits],
                    ["Page.frameNavigated", "Page.loadEventFired"],
                )
                self.assertAlmostEqual(waits[0].kwargs["timeout"], 0.8)
                self.assertAlmostEqual(waits[1].kwargs["timeout"], 0.1)


class AgentEpisodeAggregationTests(unittest.TestCase):
    def test_resource_aggregation_requires_all_workers_for_summary(self) -> None:
        samples = _aggregate_resource_samples(
            {
                "worker-1": [
                    {"timestamp": 10.0, "rss_bytes": 10, "pss_bytes": 8, "cpu_percent": 20},
                    {"timestamp": 11.0, "rss_bytes": 12, "pss_bytes": 9, "cpu_percent": 25},
                ],
                "worker-2": [
                    {"timestamp": 10.5, "rss_bytes": 20, "pss_bytes": 15, "cpu_percent": 30},
                ],
            }
        )

        self.assertEqual(samples[0]["observed_worker_count"], 1)
        self.assertEqual(samples[1]["rss_bytes"], 30)
        self.assertEqual(samples[1]["cpu_percent"], 50)

    def test_known_errors_distinguish_exact_protocol_messages_and_timeouts(self) -> None:
        counts = _known_error_counts(
            [
                {"status": "protocol_error", "cdp_error_message": "Promise was collected"},
                {"status": "timeout_page_alive", "cdp_error_message": None},
            ],
            [
                {
                    "status": "protocol_error",
                    "failure_step": 0,
                    "cdp_error_message": "NoDocumentLoaded",
                }
            ],
        )

        self.assertEqual(counts["Promise was collected"], 1)
        self.assertEqual(counts["NoDocumentLoaded"], 1)
        self.assertEqual(counts["command timeout"], 1)

    def test_known_errors_do_not_double_count_step_failure_episode_rollup(self) -> None:
        counts = _known_error_counts(
            [{"status": "timeout_page_alive", "cdp_error_message": None}],
            [
                {
                    "status": "timeout_page_alive",
                    "failure_step": 1,
                    "cdp_error_message": None,
                }
            ],
        )

        self.assertEqual(counts["command timeout"], 1)

    def test_assertion_count_comes_from_manifest_contract(self) -> None:
        self.assertEqual(
            _step_assertion_count(
                {
                    "operation": "observe",
                    "expect": {
                        "url_path": "/agent/example",
                        "text_contains": ["one", "two"],
                        "text_not_contains": ["three"],
                        "min_controls": 1,
                    },
                }
            ),
            5,
        )

    def test_unavailable_target_still_writes_authoritative_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output_dir = Path(temporary)
            summary = run_agent_episode_suite(
                output_dir=output_dir,
                target_matrix={"moli": {"available": False, "path": None}},
                targets=("moli-cdp",),
                runs=1,
                workers=1,
                parallelism=1,
                step_dwell_ms=0,
                sample_interval_ms=500,
                timeout_seconds=1,
            )
            report = json.loads(
                (output_dir / "agent-episode" / "report-data.json").read_text(
                    encoding="utf-8"
                )
            )

        self.assertEqual(summary["total_failures"], len(summary["cases"]))
        self.assertEqual(report["schema"], "moli.agent-episode.report.v1")
        self.assertTrue(all(row["status"] == "target_unavailable" for row in report["episodes"]))

    def test_report_is_self_contained_and_json_is_authoritative(self) -> None:
        summary = {
            "episodes_total": 1,
            "episodes_passed": 1,
            "total_failures": 0,
            "steps_total": 1,
            "workers": 1,
            "parallelism": 1,
            "step_dwell_ms": 14_000,
            "targets": {},
        }
        with tempfile.TemporaryDirectory() as temporary:
            suite_dir = Path(temporary)
            payload = write_agent_episode_report(
                suite_dir=suite_dir,
                summary=summary,
                episode_rows=[],
                step_rows=[],
                resources={},
                markers=[],
                config={"fixture_version": "agent-episode-fixture-v1"},
            )
            serialized = json.loads((suite_dir / "report-data.json").read_text())
            html = (suite_dir / "index.html").read_text(encoding="utf-8")

        self.assertEqual(serialized, payload)
        self.assertIn('id="report-data"', html)
        self.assertNotIn("cdn.jsdelivr.net", html)


if __name__ == "__main__":
    unittest.main()
