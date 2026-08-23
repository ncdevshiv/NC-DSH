from __future__ import annotations

import asyncio
import json
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from moli_benchmark.chrome_dcl import (
    CdpDclDumpTimeoutError,
    DEFAULT_CHROME_DCL_USER_AGENT,
    _binary_main_resource_body_from_message,
    _chrome_command,
    _recv_command_response,
    _recv_until_dcl_or_binary_main_resource,
    run_chrome_dcl_dump,
)
from moli_benchmark.raw_cdp import RawCdpClient, RawCdpError, RawCdpTimeoutError


class _NonMatchingWebSocket:
    async def send(self, _payload: str) -> None:
        return None

    async def recv(self) -> str:
        return json.dumps({"method": "Runtime.consoleAPICalled"})


class _FakeTemporaryFile:
    def __init__(self) -> None:
        self.closed = False

    def __enter__(self) -> "_FakeTemporaryFile":
        return self

    def __exit__(self, *_args: object) -> None:
        self.closed = True


class _FakeProcess:
    pid = 12345

    def __init__(self) -> None:
        self.returncode: int | None = None

    def poll(self) -> int | None:
        return self.returncode


class _RecordingCdpClient:
    def __init__(self) -> None:
        self.timeout: float | None = None

    async def recv_until_id(self, _message_id: int, *, timeout: float) -> tuple[dict[str, object], list[dict[str, object]]]:
        self.timeout = timeout
        return {"id": 1, "result": {}}, []


class _LateCommandResponseClient:
    def __init__(self, late_message: dict[str, object]) -> None:
        self.late_message = late_message

    async def recv_until_id(self, _message_id: int, *, timeout: float) -> tuple[dict[str, object], list[dict[str, object]]]:
        del timeout
        raise RawCdpTimeoutError("primary command deadline expired")

    async def recv(self) -> dict[str, object]:
        return self.late_message


def _document_response_message(
    mime_type: str,
    *,
    resource_type: str = "Document",
    status: int = 200,
) -> dict[str, object]:
    return {
        "sessionId": "SID-1",
        "method": "Network.responseReceived",
        "params": {
            "type": resource_type,
            "frameId": "FRAME-1",
            "response": {"mimeType": mime_type, "status": status},
        },
    }


class ChromeDclTests(unittest.TestCase):
    def test_chrome_command_uses_non_headless_desktop_user_agent(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            command = _chrome_command(Path("/bin/chromium"), 12345, Path(temp_dir))

        user_agent_args = [arg for arg in command if arg.startswith("--user-agent=")]
        self.assertEqual(user_agent_args, [f"--user-agent={DEFAULT_CHROME_DCL_USER_AGENT}"])
        self.assertIn("Chrome/", user_agent_args[0])
        self.assertNotIn("HeadlessChrome", user_agent_args[0])

    def test_recv_until_id_deadline_raises_timeout_error(self) -> None:
        async def run() -> None:
            client = RawCdpClient(websocket=_NonMatchingWebSocket())  # type: ignore[arg-type]
            with self.assertRaises(RawCdpTimeoutError) as raised:
                await client.recv_until_id(1, timeout=0.001)
            self.assertIsInstance(raised.exception, TimeoutError)

        asyncio.run(run())

    def test_recv_command_response_uses_remaining_deadline_without_stage_cap(self) -> None:
        async def run() -> None:
            client = _RecordingCdpClient()
            deadline = time.perf_counter() + 12.0
            await _recv_command_response(  # type: ignore[arg-type]
                client,
                1,
                deadline=deadline,
                stage="outerHTML",
            )
            self.assertIsNotNone(client.timeout)
            self.assertGreater(client.timeout or 0.0, 10.0)

        asyncio.run(run())

    def test_recv_command_response_surfaces_late_command_error_for_classification(self) -> None:
        async def run() -> None:
            client = _LateCommandResponseClient(
                {
                    "id": 7,
                    "error": {
                        "code": -32000,
                        "message": "failed to fetch page `https://example.invalid/`: curl request failed",
                    },
                }
            )

            with self.assertRaises(RawCdpError) as raised:
                await _recv_command_response(  # type: ignore[arg-type]
                    client,
                    7,
                    deadline=time.perf_counter(),
                    stage="Page.navigate",
                    late_error_grace_seconds=0.1,
                )

            self.assertIn("failed to fetch page", str(raised.exception))

        asyncio.run(run())

    def test_recv_command_response_keeps_late_success_as_timeout(self) -> None:
        async def run() -> None:
            client = _LateCommandResponseClient({"id": 7, "result": {}})

            with self.assertRaises(CdpDclDumpTimeoutError) as raised:
                await _recv_command_response(  # type: ignore[arg-type]
                    client,
                    7,
                    deadline=time.perf_counter(),
                    stage="Page.navigate",
                    late_error_grace_seconds=0.1,
                )

            self.assertEqual(raised.exception.stage, "Page.navigate")

        asyncio.run(run())

    def test_binary_main_document_response_returns_benchmark_binary_body(self) -> None:
        body = _binary_main_resource_body_from_message(
            _document_response_message("application/pdf"),
            session_id="SID-1",
            frame_id="FRAME-1",
        )

        self.assertIsNotNone(body)
        self.assertTrue(body.startswith("%PDF-"))
        self.assertIn("\0", body or "")

    def test_binary_main_document_detection_ignores_html_and_subresources(self) -> None:
        html_body = _binary_main_resource_body_from_message(
            _document_response_message("text/html; charset=utf-8"),
            session_id="SID-1",
            frame_id="FRAME-1",
        )
        script_pdf_body = _binary_main_resource_body_from_message(
            _document_response_message("application/pdf", resource_type="Script"),
            session_id="SID-1",
            frame_id="FRAME-1",
        )

        self.assertIsNone(html_body)
        self.assertIsNone(script_pdf_body)

    def test_binary_main_document_detection_ignores_error_status(self) -> None:
        body = _binary_main_resource_body_from_message(
            _document_response_message("application/pdf", status=404),
            session_id="SID-1",
            frame_id="FRAME-1",
        )

        self.assertIsNone(body)

    def test_recv_until_dcl_short_circuits_binary_document_seen_before_dcl(self) -> None:
        async def run() -> None:
            body = await _recv_until_dcl_or_binary_main_resource(
                mock.Mock(),
                session_id="SID-1",
                frame_id="FRAME-1",
                deadline=time.perf_counter() + 1.0,
                seen=[
                    _document_response_message("application/pdf"),
                    {
                        "sessionId": "SID-1",
                        "method": "Page.lifecycleEvent",
                        "params": {"frameId": "FRAME-1", "name": "DOMContentLoaded"},
                    },
                ],
            )
            self.assertIsNotNone(body)
            self.assertTrue((body or "").startswith("%PDF-"))

        asyncio.run(run())

    def test_recv_until_dcl_accepts_main_frame_event_seen_before_command_response(self) -> None:
        async def run() -> None:
            body = await _recv_until_dcl_or_binary_main_resource(
                mock.Mock(),
                session_id="SID-1",
                frame_id="FRAME-1",
                deadline=time.perf_counter() + 1.0,
                seen=[
                    {
                        "sessionId": "SID-1",
                        "method": "Page.lifecycleEvent",
                        "params": {"frameId": "FRAME-1", "name": "DOMContentLoaded"},
                    }
                ],
            )
            self.assertIsNone(body)

        asyncio.run(run())

    def test_chrome_runner_records_raw_cdp_deadline_as_timeout(self) -> None:
        process = _FakeProcess()

        def terminate(fake_process: _FakeProcess) -> None:
            fake_process.returncode = -15

        with (
            mock.patch("moli_benchmark.chrome_dcl.subprocess.Popen", return_value=process),
            mock.patch(
                "moli_benchmark.chrome_dcl._dump_dcl_html",
                side_effect=CdpDclDumpTimeoutError(
                    "DCL",
                    RawCdpTimeoutError("timed out waiting for CDP response id=1"),
                ),
            ),
            mock.patch(
                "moli_benchmark.chrome_dcl._terminate_process_group",
                side_effect=terminate,
            ),
        ):
            result = run_chrome_dcl_dump(
                Path("/bin/chromium"),
                "https://example.test/",
                timeout_seconds=1.0,
                sample_resources=False,
            )

        self.assertTrue(result.timed_out)
        self.assertEqual(result.returncode, 124)
        self.assertIn(b"chrome CDP DCL timeout", result.stderr)

    def test_chrome_runner_distinguishes_outer_html_timeout(self) -> None:
        process = _FakeProcess()

        def terminate(fake_process: _FakeProcess) -> None:
            fake_process.returncode = -15

        with (
            mock.patch("moli_benchmark.chrome_dcl.subprocess.Popen", return_value=process),
            mock.patch(
                "moli_benchmark.chrome_dcl._dump_dcl_html",
                side_effect=CdpDclDumpTimeoutError(
                    "outerHTML",
                    RawCdpTimeoutError("timed out waiting for CDP response id=9"),
                ),
            ),
            mock.patch(
                "moli_benchmark.chrome_dcl._terminate_process_group",
                side_effect=terminate,
            ),
        ):
            result = run_chrome_dcl_dump(
                Path("/bin/chromium"),
                "https://example.test/",
                timeout_seconds=1.0,
                sample_resources=False,
            )

        self.assertTrue(result.timed_out)
        self.assertEqual(result.returncode, 124)
        self.assertIn(b"chrome CDP outerHTML timeout", result.stderr)
        self.assertNotIn(b"chrome CDP DCL timeout", result.stderr)

    def test_chrome_runner_closes_tempfiles_when_popen_fails(self) -> None:
        files: list[_FakeTemporaryFile] = []

        def fake_temporary_file() -> _FakeTemporaryFile:
            file = _FakeTemporaryFile()
            files.append(file)
            return file

        with (
            mock.patch(
                "moli_benchmark.chrome_dcl.tempfile.TemporaryFile",
                side_effect=fake_temporary_file,
            ),
            mock.patch(
                "moli_benchmark.chrome_dcl.subprocess.Popen",
                side_effect=OSError("boom"),
            ),
        ):
            with self.assertRaises(OSError):
                run_chrome_dcl_dump(
                    Path("/bin/chromium"),
                    "https://example.test/",
                    timeout_seconds=1.0,
                )

        self.assertEqual(len(files), 2)
        self.assertTrue(all(file.closed for file in files))


if __name__ == "__main__":
    unittest.main()
