from __future__ import annotations

import asyncio
import os
import signal
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any

from .config import REPO_ROOT, reserve_port
from .process import ProcessResult
from .raw_cdp import RawCdpClient, RawCdpError, connect_raw_cdp
from .sampling import ResourceSampler
from .target_serve import start_target_serve, stop_target_serve


DEFAULT_CHROME_DCL_USER_AGENT = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36"
)

# Only Page.navigate uses this grace. It preserves a late protocol error that
# races the benchmark deadline, but a late successful response is still a DCL
# timeout. Keep this small so failed navigations do not distort timing reports.
CDP_LATE_ERROR_GRACE_SECONDS = 2.0


class CdpDclDumpTimeoutError(TimeoutError):
    def __init__(self, stage: str, error: BaseException) -> None:
        self.stage = stage
        self.original_error = error
        detail = str(error)
        super().__init__(f"{stage}: {detail}" if detail else stage)

    @property
    def detail(self) -> str:
        detail = str(self.original_error)
        return detail if detail else self.stage


def _chrome_command(binary: Path, port: int, profile_dir: Path) -> list[str]:
    return [
        str(binary),
        "--headless=new",
        "--disable-gpu",
        "--no-sandbox",
        "--disable-dev-shm-usage",
        "--no-first-run",
        "--no-default-browser-check",
        "--remote-debugging-address=127.0.0.1",
        f"--remote-debugging-port={port}",
        f"--user-data-dir={profile_dir}",
        f"--user-agent={DEFAULT_CHROME_DCL_USER_AGENT}",
        "about:blank",
    ]


async def _wait_for_cdp(endpoint: str, process: subprocess.Popen[bytes], timeout_seconds: float) -> RawCdpClient:
    deadline = time.perf_counter() + timeout_seconds
    last_error: Exception | None = None
    while time.perf_counter() < deadline:
        if process.poll() is not None:
            raise RawCdpError(f"chromium exited before CDP became available: rc={process.returncode}")
        try:
            return await connect_raw_cdp(endpoint)
        except Exception as error:  # noqa: BLE001 - surface the last startup failure in context.
            last_error = error
            await asyncio.sleep(0.05)
    raise TimeoutError(f"timed out waiting for Chrome CDP at {endpoint}; last_error={last_error!r}")


async def _recv_command_response(
    client: RawCdpClient,
    message_id: int,
    *,
    deadline: float,
    stage: str,
    late_error_grace_seconds: float = 0.0,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    try:
        return await client.recv_until_id(
            message_id,
            timeout=max(0.1, deadline - time.perf_counter()),
        )
    except TimeoutError as error:
        if late_error_grace_seconds > 0.0:
            await _raise_late_command_error_or_timeout(
                client,
                message_id,
                stage=stage,
                timeout_error=error,
                grace_seconds=late_error_grace_seconds,
            )
        raise CdpDclDumpTimeoutError(stage, error) from error


async def _raise_late_command_error_or_timeout(
    client: RawCdpClient,
    message_id: int,
    *,
    stage: str,
    timeout_error: BaseException,
    grace_seconds: float,
) -> None:
    """Surface a command error that arrives just after the benchmark deadline.

    The grace path is only for classification. A late successful response still
    remains a timeout, so the DCL benchmark does not accept pages that complete
    after its configured deadline.
    """
    grace_deadline = time.perf_counter() + grace_seconds
    while True:
        remaining = grace_deadline - time.perf_counter()
        if remaining <= 0.0:
            raise CdpDclDumpTimeoutError(stage, timeout_error) from timeout_error
        try:
            message = await asyncio.wait_for(client.recv(), timeout=remaining)
        except TimeoutError:
            raise CdpDclDumpTimeoutError(stage, timeout_error) from timeout_error
        if message.get("id") != message_id:
            continue
        if "error" in message:
            raise RawCdpError(f"CDP command id={message_id} failed: {message['error']}")
        raise CdpDclDumpTimeoutError(stage, timeout_error) from timeout_error


def _is_dcl_event(message: dict[str, Any], session_id: str, frame_id: str | None) -> bool:
    if message.get("sessionId") != session_id:
        return False
    method = message.get("method")
    if method == "Page.domContentEventFired":
        return True
    if method != "Page.lifecycleEvent":
        return False
    params = message.get("params")
    if not isinstance(params, dict):
        return False
    if frame_id is not None and params.get("frameId") != frame_id:
        return False
    return params.get("name") in {"DOMContentLoaded", "domContentLoaded"}


_BINARY_DOCUMENT_MIME_PREFIXES = (
    "audio/",
    "font/",
    "image/",
    "video/",
)

_BINARY_DOCUMENT_MIME_TYPES = {
    "application/gzip",
    "application/octet-stream",
    "application/pdf",
    "application/vnd.ms-fontobject",
    "application/x-7z-compressed",
    "application/x-bzip2",
    "application/x-gzip",
    "application/x-rar-compressed",
    "application/x-tar",
    "application/zip",
}


def _is_binary_document_mime_type(mime_type: str) -> bool:
    normalized = mime_type.split(";", 1)[0].strip().lower()
    return normalized in _BINARY_DOCUMENT_MIME_TYPES or normalized.startswith(
        _BINARY_DOCUMENT_MIME_PREFIXES
    )


def _binary_main_resource_body(mime_type: str) -> str:
    normalized = mime_type.split(";", 1)[0].strip().lower()
    if normalized == "application/pdf":
        return "%PDF-1.7\n% moli benchmark CDP binary main resource\n" + ("\0" * 512)
    return f"moli benchmark CDP binary main resource: {normalized}\n" + ("\0" * 512)


def _binary_main_resource_body_from_message(
    message: dict[str, Any],
    *,
    session_id: str,
    frame_id: str | None,
) -> str | None:
    if message.get("sessionId") != session_id:
        return None
    if message.get("method") != "Network.responseReceived":
        return None
    params = message.get("params")
    if not isinstance(params, dict):
        return None
    if params.get("type") != "Document":
        return None
    if frame_id is not None and params.get("frameId") != frame_id:
        return None
    response = params.get("response")
    if not isinstance(response, dict):
        return None
    status = response.get("status")
    if isinstance(status, (int, float)) and not 200 <= status < 400:
        return None
    mime_type = response.get("mimeType")
    if not isinstance(mime_type, str) or not _is_binary_document_mime_type(mime_type):
        return None
    return _binary_main_resource_body(mime_type)


async def _recv_until_dcl_or_binary_main_resource(
    client: RawCdpClient,
    *,
    session_id: str,
    frame_id: str | None,
    deadline: float,
    seen: list[dict[str, Any]],
) -> str | None:
    for message in seen:
        binary_body = _binary_main_resource_body_from_message(
            message,
            session_id=session_id,
            frame_id=frame_id,
        )
        if binary_body is not None:
            return binary_body
    if any(_is_dcl_event(message, session_id, frame_id) for message in seen):
        return None
    while True:
        remaining = deadline - time.perf_counter()
        if remaining <= 0:
            raise TimeoutError("timed out waiting for Page.domContentEventFired")
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        binary_body = _binary_main_resource_body_from_message(
            message,
            session_id=session_id,
            frame_id=frame_id,
        )
        if binary_body is not None:
            return binary_body
        if _is_dcl_event(message, session_id, frame_id):
            return None


async def _dump_dcl_html(endpoint: str, process: subprocess.Popen[bytes], url: str, timeout_seconds: float) -> str:
    deadline = time.perf_counter() + timeout_seconds
    try:
        client = await _wait_for_cdp(endpoint, process, min(5.0, max(0.1, timeout_seconds)))
    except TimeoutError as error:
        raise CdpDclDumpTimeoutError("startup", error) from error
    target_id: str | None = None
    try:
        create_id = await client.send("Target.createTarget", {"url": "about:blank"})
        create_response, _ = await _recv_command_response(
            client,
            create_id,
            deadline=deadline,
            stage="Target.createTarget",
        )
        target_id = str(create_response["result"]["targetId"])

        attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
        attach_response, _ = await _recv_command_response(
            client,
            attach_id,
            deadline=deadline,
            stage="Target.attachToTarget",
        )
        session_id = str(attach_response["result"]["sessionId"])

        for method in ("Page.enable", "Runtime.enable", "Network.enable"):
            message_id = await client.send(method, session_id=session_id)
            await _recv_command_response(
                client,
                message_id,
                deadline=deadline,
                stage=method,
            )
        lifecycle_id = await client.send("Page.setLifecycleEventsEnabled", {"enabled": True}, session_id=session_id)
        await _recv_command_response(
            client,
            lifecycle_id,
            deadline=deadline,
            stage="Page.setLifecycleEventsEnabled",
        )

        navigate_id = await client.send("Page.navigate", {"url": url}, session_id=session_id)
        navigate_response, seen = await _recv_command_response(
            client,
            navigate_id,
            deadline=deadline,
            stage="Page.navigate",
            late_error_grace_seconds=CDP_LATE_ERROR_GRACE_SECONDS,
        )
        frame_id = navigate_response.get("result", {}).get("frameId")
        if frame_id is not None:
            frame_id = str(frame_id)
        try:
            binary_body = await _recv_until_dcl_or_binary_main_resource(
                client,
                session_id=session_id,
                frame_id=frame_id,
                deadline=deadline,
                seen=seen,
            )
        except TimeoutError as error:
            raise CdpDclDumpTimeoutError("DCL", error) from error
        if binary_body is not None:
            return binary_body

        evaluate_id = await client.send(
            "Runtime.evaluate",
            {
                "expression": "document.documentElement ? document.documentElement.outerHTML : ''",
                "returnByValue": True,
            },
            session_id=session_id,
        )
        evaluate_response, _ = await _recv_command_response(
            client,
            evaluate_id,
            deadline=deadline,
            stage="outerHTML",
        )
        result = evaluate_response.get("result", {}).get("result", {})
        value = result.get("value", "")
        return value if isinstance(value, str) else ""
    finally:
        if target_id is not None:
            try:
                close_id = await client.send("Target.closeTarget", {"targetId": target_id})
                await client.recv_until_id(close_id, timeout=1.0)
            except Exception:
                pass
        await client.websocket.close()


def _terminate_process_group(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except OSError:
        pass
    try:
        process.wait(timeout=2)
        return
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except OSError:
        pass
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        pass


def _read_tempfile(file_obj: Any) -> bytes:
    file_obj.flush()
    file_obj.seek(0)
    return file_obj.read()


def _process_returncode_or(process: subprocess.Popen[bytes], fallback: int) -> int:
    returncode = process.poll()
    return int(returncode) if returncode is not None else fallback


def run_chrome_dcl_dump(
    binary: Path,
    url: str,
    *,
    cwd: Path = REPO_ROOT,
    timeout_seconds: float,
    env: dict[str, str] | None = None,
    sample_resources: bool = True,
) -> ProcessResult:
    started = time.perf_counter()
    command: list[str] = []
    stdout = b""
    stderr = b""
    browser_stdout = b""
    error_suffix = b""
    timed_out = False
    returncode: int | None = None
    with tempfile.TemporaryDirectory(prefix="moli-benchmark-chrome-") as temp_dir:
        profile_dir = Path(temp_dir) / "profile"
        profile_dir.mkdir()
        with tempfile.TemporaryFile() as stdout_file, tempfile.TemporaryFile() as stderr_file:
            reserved_port = reserve_port()
            try:
                port = reserved_port.port
                command = _chrome_command(binary, port, profile_dir)
                endpoint = f"http://127.0.0.1:{port}"
                reserved_port.release_socket()
                process = subprocess.Popen(
                    command,
                    cwd=cwd,
                    env=env,
                    stdout=stdout_file,
                    stderr=stderr_file,
                    start_new_session=True,
                )
            except BaseException:
                reserved_port.close()
                raise
            sampler = ResourceSampler(process.pid) if sample_resources else None
            if sampler is not None:
                sampler.start()
            try:
                try:
                    html = asyncio.run(_dump_dcl_html(endpoint, process, url, timeout_seconds))
                    stdout = html.encode("utf-8", errors="replace")
                    returncode = 0
                except CdpDclDumpTimeoutError as error:
                    timed_out = True
                    returncode = _process_returncode_or(process, 124)
                    error_suffix = (
                        f"\nchrome CDP {error.stage} timeout: {error.detail}\n"
                    ).encode("utf-8", errors="replace")
                except TimeoutError as error:
                    timed_out = True
                    returncode = _process_returncode_or(process, 124)
                    error_suffix = f"\nchrome CDP timeout: {error}\n".encode("utf-8", errors="replace")
                except Exception as error:  # noqa: BLE001 - convert CDP/browser failures into benchmark process output.
                    returncode = _process_returncode_or(process, 1)
                    error_suffix = f"\nchrome CDP DCL error: {error}\n".encode("utf-8", errors="replace")
                finally:
                    _terminate_process_group(process)
                    reserved_port.close()
                    if returncode is None:
                        returncode = process.returncode
                    browser_stdout = _read_tempfile(stdout_file)
                    stderr = _read_tempfile(stderr_file) + error_suffix
            finally:
                elapsed_ms = (time.perf_counter() - started) * 1000.0
                resources = sampler.stop() if sampler is not None else {}
    if browser_stdout.strip() and not stdout:
        stdout = browser_stdout
    return ProcessResult(
        command=command,
        returncode=returncode,
        elapsed_ms=elapsed_ms,
        stdout=stdout,
        stderr=stderr,
        timed_out=timed_out,
        resources=resources,
    )


def run_served_cdp_dcl_dump(
    target: str,
    binary: Path,
    url: str,
    *,
    cwd: Path = REPO_ROOT,
    timeout_seconds: float,
    env: dict[str, str] | None = None,
) -> ProcessResult:
    del cwd, env
    started = time.perf_counter()
    command: list[str] = [str(binary), "serve"]
    stdout = b""
    stderr = b""
    error_suffix = b""
    timed_out = False
    returncode: int | None = None
    resources: dict[str, Any] = {}
    serve = None
    try:
        serve = start_target_serve(target, binary, timeout_seconds)
        command = serve.command
        try:
            html = asyncio.run(_dump_dcl_html(serve.endpoint, serve.process, url, timeout_seconds))
            stdout = html.encode("utf-8", errors="replace")
            returncode = 0
        except CdpDclDumpTimeoutError as error:
            timed_out = True
            returncode = _process_returncode_or(serve.process, 124)
            error_suffix = (
                f"\n{target} CDP {error.stage} timeout: {error.detail}\n"
            ).encode("utf-8", errors="replace")
        except TimeoutError as error:
            timed_out = True
            returncode = _process_returncode_or(serve.process, 124)
            error_suffix = f"\n{target} CDP timeout: {error}\n".encode("utf-8", errors="replace")
        except Exception as error:  # noqa: BLE001 - convert CDP/browser failures into benchmark process output.
            returncode = _process_returncode_or(serve.process, 1)
            error_suffix = f"\n{target} CDP DCL error: {error}\n".encode("utf-8", errors="replace")
        finally:
            stopped = stop_target_serve(serve)
            serve = None
            if returncode is None:
                stopped_returncode = stopped.get("returncode")
                returncode = int(stopped_returncode) if isinstance(stopped_returncode, int) else None
            stopped_resources = stopped.get("resources")
            resources = stopped_resources if isinstance(stopped_resources, dict) else {}
            log_tail = stopped.get("log_tail")
            if isinstance(log_tail, list) and log_tail:
                stderr = "\n".join(str(line) for line in log_tail).encode("utf-8", errors="replace")
            stderr += error_suffix
    except TimeoutError as error:
        timed_out = True
        error_suffix = f"\n{target} CDP timeout: {error}\n".encode("utf-8", errors="replace")
        stderr += error_suffix
    except Exception as error:  # noqa: BLE001 - startup failures are benchmark process errors.
        error_suffix = f"\n{target} CDP DCL error: {error}\n".encode("utf-8", errors="replace")
        stderr += error_suffix
    finally:
        if serve is not None:
            stopped = stop_target_serve(serve)
            stopped_resources = stopped.get("resources")
            if isinstance(stopped_resources, dict) and not resources:
                resources = stopped_resources
        elapsed_ms = (time.perf_counter() - started) * 1000.0
    return ProcessResult(
        command=command,
        returncode=returncode,
        elapsed_ms=elapsed_ms,
        stdout=stdout,
        stderr=stderr,
        timed_out=timed_out,
        resources=resources,
    )
