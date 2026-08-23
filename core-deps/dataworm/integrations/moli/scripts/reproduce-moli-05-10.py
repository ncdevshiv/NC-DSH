#!/usr/bin/env python3
from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request
from collections import Counter
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "moli-benchmark"))

from moli_benchmark.config import clear_proxy_env, moli_binary, reserve_port
from moli_benchmark.raw_cdp import connect_raw_cdp


class ReproError(RuntimeError):
    pass


class FixtureHandler(BaseHTTPRequestHandler):
    server_version = "MoliReproFixture/1.0"

    def log_message(self, format: str, *args: Any) -> None:
        return

    def _send(self, status: int, content_type: str, body: str) -> None:
        encoded = body.encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", content_type)
        self.send_header("cache-control", "no-store")
        self.send_header("content-length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def do_GET(self) -> None:
        path = self.path.split("?", 1)[0]
        if path == "/favicon.ico":
            self.send_response(204)
            self.end_headers()
            return
        if path == "/ax":
            self._send(
                200,
                "text/html; charset=utf-8",
                """<!doctype html>
<html>
<head><title>AX Repro Title</title></head>
<body>
  <main>
    <h1>AX Repro Heading</h1>
    <p>Visible paragraph text</p>
    <button id="submit">Submit Order</button>
    <a href="/next">Read more</a>
  </main>
</body>
</html>""",
            )
            return
        if path == "/route-abort":
            self._send(
                200,
                "text/html; charset=utf-8",
                """<!doctype html>
<html>
<body data-app-loaded="false">
  <script src="/app.js"></script>
  <main>route abort fixture</main>
</body>
</html>""",
            )
            return
        if path == "/app.js":
            self._send(
                200,
                "application/javascript; charset=utf-8",
                "globalThis.__appLoaded = true; document.body.dataset.appLoaded = 'true';",
            )
            return
        self._send(404, "text/plain; charset=utf-8", "not found")


@dataclass
class FixtureServer:
    server: ThreadingHTTPServer
    thread: threading.Thread
    url: str

    def close(self) -> None:
        self.server.shutdown()
        self.thread.join(timeout=2)
        self.server.server_close()


def start_fixture_server() -> FixtureServer:
    server = ThreadingHTTPServer(("127.0.0.1", 0), FixtureHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    host, port = server.server_address
    return FixtureServer(server=server, thread=thread, url=f"http://{host}:{port}")


@dataclass
class Serve:
    process: asyncio.subprocess.Process
    endpoint: str
    logs: list[str]
    tasks: list[asyncio.Task[Any]]
    port_lease: Any


async def _collect_output(stream: asyncio.StreamReader | None, logs: list[str], label: str) -> None:
    if stream is None:
        return
    while True:
        line = await stream.readline()
        if not line:
            return
        logs.append(f"{label}: {line.decode('utf-8', errors='replace').rstrip()}")
        if len(logs) > 200:
            del logs[: len(logs) - 200]


def _probe_url(url: str) -> bool:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        with opener.open(url, timeout=0.5) as response:
            response.read()
        return True
    except (TimeoutError, OSError, urllib.error.URLError):
        return False


async def start_moli(binary: Path, *, timeout: float) -> Serve:
    port_lease = reserve_port()
    port = port_lease.port
    endpoint = f"http://127.0.0.1:{port}"
    port_lease.release_socket()
    process = await asyncio.create_subprocess_exec(
        str(binary),
        "serve",
        "--host",
        "127.0.0.1",
        "--port",
        str(port),
        "--log-level",
        "warn",
        cwd=str(REPO_ROOT),
        env=clear_proxy_env(os.environ),
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    logs: list[str] = []
    tasks = [
        asyncio.create_task(_collect_output(process.stdout, logs, "stdout")),
        asyncio.create_task(_collect_output(process.stderr, logs, "stderr")),
    ]
    serve = Serve(process=process, endpoint=endpoint, logs=logs, tasks=tasks, port_lease=port_lease)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.returncode is not None:
            raise ReproError(f"moli exited early with {process.returncode}: {logs[-20:]}")
        if await asyncio.to_thread(_probe_url, f"{endpoint}/json/version"):
            return serve
        await asyncio.sleep(0.05)
    await stop_moli(serve)
    raise ReproError(f"timed out waiting for moli serve at {endpoint}: {logs[-20:]}")


async def stop_moli(serve: Serve | None) -> dict[str, Any]:
    if serve is None:
        return {}
    try:
        if serve.process.returncode is None:
            serve.process.terminate()
            try:
                await asyncio.wait_for(serve.process.wait(), timeout=3)
            except asyncio.TimeoutError:
                serve.process.kill()
                await serve.process.wait()
    finally:
        for task in serve.tasks:
            task.cancel()
        await asyncio.gather(*serve.tasks, return_exceptions=True)
        serve.port_lease.close()
    return {"returncode": serve.process.returncode, "log_tail": serve.logs[-40:]}


async def recv_until_id(
    client: Any,
    message_id: int,
    *,
    timeout: float,
    allow_error: bool = True,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    seen: list[dict[str, Any]] = []
    deadline = asyncio.get_running_loop().time() + timeout
    while True:
        remaining = deadline - asyncio.get_running_loop().time()
        if remaining <= 0:
            raise TimeoutError(f"timed out waiting for CDP response id={message_id}; seen={seen[-20:]}")
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        seen.append(message)
        if message.get("id") == message_id:
            if not allow_error and "error" in message:
                raise ReproError(f"CDP command id={message_id} failed: {message['error']}")
            return message, seen


async def create_page_session(
    client: Any,
    url: str,
    *,
    enable_runtime: bool = True,
    enable_network: bool = False,
    lifecycle: bool = False,
    wait_for: str = "load",
    timeout: float = 15.0,
) -> tuple[str, str, list[dict[str, Any]]]:
    create_id = await client.send("Target.createTarget", {"url": "about:blank"})
    create, _ = await recv_until_id(client, create_id, timeout=timeout, allow_error=False)
    target_id = create.get("result", {}).get("targetId")
    if not isinstance(target_id, str) or not target_id:
        raise ReproError(f"missing targetId in Target.createTarget response: {create}")

    attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
    attach, _ = await recv_until_id(client, attach_id, timeout=timeout, allow_error=False)
    session_id = attach.get("result", {}).get("sessionId")
    if not isinstance(session_id, str) or not session_id:
        raise ReproError(f"missing sessionId in Target.attachToTarget response: {attach}")

    for method in ["Page.enable", *(["Runtime.enable"] if enable_runtime else []), *(["Network.enable"] if enable_network else [])]:
        command_id = await client.send(method, session_id=session_id)
        await recv_until_id(client, command_id, timeout=timeout, allow_error=False)

    if lifecycle:
        lifecycle_id = await client.send(
            "Page.setLifecycleEventsEnabled",
            {"enabled": True},
            session_id=session_id,
        )
        await recv_until_id(client, lifecycle_id, timeout=timeout, allow_error=False)

    seen = await navigate(client, session_id, url, wait_for=wait_for, timeout=timeout)
    return session_id, target_id, seen


async def navigate(
    client: Any,
    session_id: str,
    url: str,
    *,
    wait_for: str = "load",
    timeout: float = 15.0,
) -> list[dict[str, Any]]:
    navigate_id = await client.send("Page.navigate", {"url": url}, session_id=session_id)
    seen: list[dict[str, Any]] = []
    saw_response = False
    saw_boundary = wait_for == "none"
    deadline = asyncio.get_running_loop().time() + timeout
    while not (saw_response and saw_boundary):
        remaining = deadline - asyncio.get_running_loop().time()
        if remaining <= 0:
            if saw_response:
                return seen
            raise TimeoutError(f"timed out waiting for Page.navigate response; seen={seen[-20:]}")
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        seen.append(message)
        if message.get("id") == navigate_id:
            if "error" in message:
                raise ReproError(f"Page.navigate failed: {message['error']}")
            saw_response = True
        method = message.get("method")
        if message.get("sessionId") == session_id:
            if wait_for == "dcl" and method == "Page.domContentEventFired":
                saw_boundary = True
            if wait_for == "load" and method == "Page.loadEventFired":
                saw_boundary = True
    return seen


def ax_field(node: dict[str, Any], field: str) -> str:
    value = node.get(field)
    if isinstance(value, dict) and isinstance(value.get("value"), str):
        return value["value"]
    return ""


async def moli_05(client: Any, fixture: str) -> dict[str, Any]:
    session_id, _target_id, _ = await create_page_session(client, f"{fixture}/ax", timeout=12)
    command_id = await client.send("Accessibility.getFullAXTree", session_id=session_id)
    response, _ = await recv_until_id(client, command_id, timeout=10, allow_error=True)
    if "error" in response:
        return {"status": "reproduced", "error": response["error"]}
    nodes = response.get("result", {}).get("nodes") or []
    role_counts = Counter(ax_field(node, "role") for node in nodes if isinstance(node, dict))
    names = [ax_field(node, "name") for node in nodes if isinstance(node, dict) and ax_field(node, "name")]
    expected_texts = [
        "AX Repro Title",
        "AX Repro Heading",
        "Visible paragraph text",
        "Submit Order",
        "Read more",
    ]
    present = {text: text in names for text in expected_texts}
    generic_or_none_empty = sum(
        1
        for node in nodes
        if isinstance(node, dict)
        and ax_field(node, "role") in {"none", "generic"}
        and not ax_field(node, "name")
    )
    missing_count = sum(1 for ok in present.values() if not ok)
    status = "reproduced" if not names or missing_count >= 3 else "not_reproduced"
    return {
        "status": status,
        "node_count": len(nodes),
        "role_counts": dict(role_counts),
        "name_samples": names[:20],
        "expected_text_present": present,
        "generic_or_none_empty": generic_or_none_empty,
    }


def _string_index_valid(value: Any, strings: list[Any]) -> bool:
    return isinstance(value, int) and 0 <= value < len(strings)


def validate_dom_snapshot(result: dict[str, Any], computed_styles: list[str]) -> list[str]:
    issues: list[str] = []
    strings = result.get("strings")
    if not isinstance(strings, list):
        return ["missing strings table"]
    documents = result.get("documents")
    if not isinstance(documents, list) or not documents:
        return ["missing documents array"]
    for doc_index, document in enumerate(documents):
        if not isinstance(document, dict):
            issues.append(f"documents[{doc_index}] is not an object")
            continue
        for field in ["documentURL", "title", "baseURL", "contentLanguage", "encodingName", "publicId", "systemId", "frameId"]:
            if not _string_index_valid(document.get(field), strings):
                issues.append(f"documents[{doc_index}].{field} is not a valid string index: {document.get(field)!r}")
        nodes = document.get("nodes")
        if not isinstance(nodes, dict):
            issues.append(f"documents[{doc_index}].nodes missing")
            continue
        required = ["parentIndex", "nodeType", "nodeName", "nodeValue", "backendNodeId", "attributes"]
        arrays: dict[str, list[Any]] = {}
        for field in required:
            value = nodes.get(field)
            if not isinstance(value, list):
                issues.append(f"documents[{doc_index}].nodes.{field} is not an array")
            else:
                arrays[field] = value
        node_count = len(arrays.get("nodeName", []))
        for field, value in arrays.items():
            if len(value) != node_count:
                issues.append(f"nodes.{field} length {len(value)} != node_count {node_count}")
        for field in ["nodeName", "nodeValue"]:
            for index, value in enumerate(arrays.get(field, [])):
                if not _string_index_valid(value, strings):
                    issues.append(f"nodes.{field}[{index}] invalid string index {value!r}")
                    break
        for index, value in enumerate(arrays.get("parentIndex", [])):
            if not isinstance(value, int) or not (-1 <= value < node_count):
                issues.append(f"nodes.parentIndex[{index}] invalid node index {value!r}")
                break
        for index, value in enumerate(arrays.get("attributes", [])):
            if not isinstance(value, list):
                issues.append(f"nodes.attributes[{index}] is not an array")
                break
            if len(value) % 2 != 0:
                issues.append(f"nodes.attributes[{index}] has odd length {len(value)}")
                break
            bad = next((item for item in value if not _string_index_valid(item, strings)), None)
            if bad is not None:
                issues.append(f"nodes.attributes[{index}] invalid string index {bad!r}")
                break
        layout = document.get("layout")
        if not isinstance(layout, dict):
            issues.append(f"documents[{doc_index}].layout missing")
            continue
        node_index = layout.get("nodeIndex")
        if not isinstance(node_index, list):
            issues.append("layout.nodeIndex is not an array")
            continue
        layout_count = len(node_index)
        for index, value in enumerate(node_index):
            if not isinstance(value, int) or not (0 <= value < node_count):
                issues.append(f"layout.nodeIndex[{index}] invalid node index {value!r}")
                break
        for field in ["styles", "bounds", "text"]:
            value = layout.get(field)
            if not isinstance(value, list):
                issues.append(f"layout.{field} is not an array")
            elif len(value) != layout_count:
                issues.append(f"layout.{field} length {len(value)} != layout_count {layout_count}")
        for index, style in enumerate(layout.get("styles") or []):
            if not isinstance(style, list) or len(style) != len(computed_styles):
                issues.append(f"layout.styles[{index}] shape mismatch: {style!r}")
                break
            bad = next((item for item in style if not _string_index_valid(item, strings)), None)
            if bad is not None:
                issues.append(f"layout.styles[{index}] invalid string index {bad!r}")
                break
        for field in ["bounds", "offsetRects", "scrollRects", "clientRects"]:
            if field not in layout:
                continue
            for index, bounds in enumerate(layout.get(field) or []):
                if not isinstance(bounds, list) or len(bounds) != 4 or not all(isinstance(item, (int, float)) for item in bounds):
                    issues.append(f"layout.{field}[{index}] invalid bounds {bounds!r}")
                    break
        for index, value in enumerate(layout.get("text") or []):
            if not _string_index_valid(value, strings):
                issues.append(f"layout.text[{index}] invalid string index {value!r}")
                break
        text_boxes = document.get("textBoxes")
        if isinstance(text_boxes, dict):
            box_arrays = [text_boxes.get(field) for field in ["layoutIndex", "bounds", "start", "length"]]
            if all(isinstance(value, list) for value in box_arrays):
                box_count = len(box_arrays[0])
                for field, value in zip(["layoutIndex", "bounds", "start", "length"], box_arrays, strict=True):
                    if len(value) != box_count:
                        issues.append(f"textBoxes.{field} length {len(value)} != text box count {box_count}")
                for index, value in enumerate(box_arrays[0]):
                    if not isinstance(value, int) or not (0 <= value < layout_count):
                        issues.append(f"textBoxes.layoutIndex[{index}] invalid layout index {value!r}")
                        break
            else:
                issues.append("textBoxes parallel arrays missing")
    return issues


async def moli_06(client: Any, url: str) -> dict[str, Any]:
    session_id, _target_id, nav_seen = await create_page_session(client, url, timeout=20)
    computed_styles = ["display", "color"]
    command_id = await client.send(
        "DOMSnapshot.captureSnapshot",
        {
            "computedStyles": computed_styles,
            "includePaintOrder": True,
            "includeDOMRects": True,
            "includeBlendedBackgroundColors": True,
            "includeTextColorOpacities": True,
        },
        session_id=session_id,
    )
    response, _ = await recv_until_id(client, command_id, timeout=15, allow_error=True)
    if "error" in response:
        return {"status": "reproduced", "error": response["error"], "navigation_events": summarize_events(nav_seen)}
    result = response.get("result")
    if not isinstance(result, dict):
        return {"status": "reproduced", "error": "missing result object", "response": response}
    issues = validate_dom_snapshot(result, computed_styles)
    document = (result.get("documents") or [{}])[0]
    nodes = document.get("nodes") if isinstance(document, dict) else {}
    layout = document.get("layout") if isinstance(document, dict) else {}
    return {
        "status": "reproduced" if issues else "not_reproduced",
        "issues": issues,
        "strings": len(result.get("strings") or []),
        "node_count": len((nodes or {}).get("nodeName") or []),
        "layout_count": len((layout or {}).get("nodeIndex") or []),
        "navigation_events": summarize_events(nav_seen),
    }


def summarize_events(messages: list[dict[str, Any]]) -> dict[str, int]:
    counts = Counter(message.get("method") for message in messages if message.get("method"))
    return dict(counts)


async def heavy_command(client: Any, url: str, method: str, timeout: float) -> dict[str, Any]:
    session_id, _target_id, nav_seen = await create_page_session(
        client,
        url,
        enable_runtime=True,
        enable_network=True,
        wait_for="load",
        timeout=timeout,
    )
    html_length = await evaluate_outer_html_length(client, session_id)
    params: dict[str, Any] | None = None
    if method == "DOMSnapshot.captureSnapshot":
        params = {"computedStyles": ["display"]}
    command_id = await client.send(method, params, session_id=session_id)
    started = time.perf_counter()
    try:
        response, seen = await recv_until_id(client, command_id, timeout=timeout, allow_error=True)
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        status = "reproduced" if "error" in response else "not_reproduced"
        result = response.get("result") if isinstance(response.get("result"), dict) else {}
        shape_summary: dict[str, Any] = {}
        if method == "DOMSnapshot.captureSnapshot" and isinstance(result, dict):
            document = (result.get("documents") or [{}])[0]
            if isinstance(document, dict):
                nodes = document.get("nodes") or {}
                layout = document.get("layout") or {}
                shape_summary = {
                    "strings": len(result.get("strings") or []),
                    "node_count": len(nodes.get("nodeName") or []) if isinstance(nodes, dict) else 0,
                    "layout_count": len(layout.get("nodeIndex") or []) if isinstance(layout, dict) else 0,
                }
        if method == "Accessibility.getFullAXTree" and isinstance(result, dict):
            nodes = result.get("nodes") or []
            shape_summary = {"node_count": len(nodes) if isinstance(nodes, list) else 0}
        return {
            "status": status,
            "elapsed_ms": round(elapsed_ms, 1),
            "outer_html_length": html_length,
            "response_error": response.get("error"),
            "result_keys": sorted(result.keys()) if isinstance(result, dict) else [],
            "shape_summary": shape_summary,
            "events_while_waiting": summarize_events(seen),
            "navigation_events": summarize_events(nav_seen),
        }
    except Exception as error:
        heartbeat: dict[str, Any] = {"ok": False, "error": repr(error)}
        try:
            heartbeat_id = await client.send("Browser.getVersion")
            heartbeat_response, _ = await recv_until_id(client, heartbeat_id, timeout=3, allow_error=True)
            heartbeat = {"ok": "error" not in heartbeat_response, "response": heartbeat_response}
        except Exception as heartbeat_error:
            heartbeat = {"ok": False, "error": repr(heartbeat_error)}
        return {
            "status": "reproduced",
            "failure": repr(error),
            "outer_html_length": html_length,
            "heartbeat_after_failure": heartbeat,
            "navigation_events": summarize_events(nav_seen),
        }


async def evaluate_outer_html_length(client: Any, session_id: str) -> int | None:
    try:
        evaluate_id = await client.send(
            "Runtime.evaluate",
            {
                "expression": "document.documentElement ? document.documentElement.outerHTML.length : 0",
                "returnByValue": True,
            },
            session_id=session_id,
        )
        response, _ = await recv_until_id(client, evaluate_id, timeout=5, allow_error=True)
        value = response.get("result", {}).get("result", {}).get("value")
        return value if isinstance(value, int) else None
    except Exception:
        return None


async def moli_07(client: Any, url: str, timeout: float) -> dict[str, Any]:
    result = await heavy_command(client, url, "DOMSnapshot.captureSnapshot", timeout)
    return {
        "status": result["status"],
        "target_url": url,
        "note": "Each heavy command is run in a separate moli process by the top-level runner.",
        "result": result,
    }


async def moli_07_ax(client: Any, url: str, timeout: float) -> dict[str, Any]:
    result = await heavy_command(client, url, "Accessibility.getFullAXTree", timeout)
    return {
        "status": result["status"],
        "target_url": url,
        "result": result,
    }


async def moli_08(client: Any, fixture: str) -> dict[str, Any]:
    session_id, _target_id, _ = await create_page_session(
        client,
        "about:blank",
        lifecycle=True,
        wait_for="none",
        timeout=10,
    )
    navigate_id = await client.send("Page.navigate", {"url": f"{fixture}/ax"}, session_id=session_id)
    seen: list[dict[str, Any]] = []
    saw_response = False
    saw_load = False
    saw_lifecycle_load = False
    deadline = asyncio.get_running_loop().time() + 12
    while not (saw_response and saw_load and saw_lifecycle_load):
        remaining = deadline - asyncio.get_running_loop().time()
        if remaining <= 0:
            break
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        seen.append(message)
        if message.get("id") == navigate_id:
            saw_response = True
        if message.get("sessionId") == session_id and message.get("method") == "Page.loadEventFired":
            saw_load = True
        if (
            message.get("sessionId") == session_id
            and message.get("method") == "Page.lifecycleEvent"
            and message.get("params", {}).get("name") == "load"
        ):
            saw_lifecycle_load = True
    lifecycle_names = [
        message.get("params", {}).get("name")
        for message in seen
        if message.get("sessionId") == session_id and message.get("method") == "Page.lifecycleEvent"
    ]
    page_events = [
        message.get("method")
        for message in seen
        if message.get("sessionId") == session_id
        and message.get("method") in {"Page.domContentEventFired", "Page.loadEventFired"}
    ]
    missing_lifecycle_load = saw_load and "load" not in lifecycle_names
    return {
        "status": "reproduced" if not lifecycle_names else "not_reproduced",
        "lifecycle_names": lifecycle_names,
        "missing_lifecycle_load": missing_lifecycle_load,
        "page_events": page_events,
        "saw_navigate_response": saw_response,
        "saw_load": saw_load,
    }


async def moli_09(client: Any, fixture: str) -> dict[str, Any]:
    session_id, _target_id, _ = await create_page_session(
        client,
        "about:blank",
        enable_runtime=True,
        enable_network=True,
        wait_for="none",
        timeout=10,
    )
    fetch_id = await client.send(
        "Fetch.enable",
        {"patterns": [{"urlPattern": "*app.js*", "requestStage": "Request"}]},
        session_id=session_id,
    )
    await recv_until_id(client, fetch_id, timeout=5, allow_error=False)
    navigate_id = await client.send("Page.navigate", {"url": f"{fixture}/route-abort"}, session_id=session_id)
    seen: list[dict[str, Any]] = []
    paused_url: str | None = None
    fail_id: int | None = None
    fail_response: dict[str, Any] | None = None
    saw_navigation_response = False
    saw_load = False
    loading_failed = False
    deadline = asyncio.get_running_loop().time() + 15
    while not (saw_navigation_response and saw_load and (fail_id is None or fail_response is not None)):
        remaining = deadline - asyncio.get_running_loop().time()
        if remaining <= 0:
            break
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        seen.append(message)
        if message.get("id") == navigate_id:
            saw_navigation_response = True
        if fail_id is not None and message.get("id") == fail_id:
            fail_response = message
        if message.get("sessionId") != session_id:
            continue
        method = message.get("method")
        params = message.get("params") or {}
        if method == "Fetch.requestPaused" and paused_url is None:
            request = params.get("request") or {}
            url = request.get("url")
            if isinstance(url, str) and url.endswith("/app.js"):
                paused_url = url
                fail_id = await client.send(
                    "Fetch.failRequest",
                    {"requestId": params.get("requestId"), "errorReason": "BlockedByClient"},
                    session_id=session_id,
                )
        if method == "Network.loadingFailed":
            request = params.get("requestId")
            error_text = params.get("errorText")
            if error_text == "net::ERR_BLOCKED_BY_CLIENT" or request:
                loading_failed = loading_failed or error_text == "net::ERR_BLOCKED_BY_CLIENT"
        if method == "Page.loadEventFired":
            saw_load = True

    eval_id = await client.send(
        "Runtime.evaluate",
        {
            "expression": "({ appLoaded: Boolean(globalThis.__appLoaded), bodyFlag: document.body && document.body.dataset.appLoaded })",
            "returnByValue": True,
        },
        session_id=session_id,
    )
    eval_response, _ = await recv_until_id(client, eval_id, timeout=5, allow_error=True)
    value = eval_response.get("result", {}).get("result", {}).get("value") or {}
    app_loaded = bool(value.get("appLoaded"))
    intercepted = paused_url is not None and fail_response is not None and "error" not in fail_response
    status = "not_reproduced" if intercepted and not app_loaded else "reproduced"
    return {
        "status": status,
        "paused_url": paused_url,
        "fail_response": fail_response,
        "loading_failed_blocked_by_client": loading_failed,
        "app_loaded": app_loaded,
        "body_flag": value.get("bodyFlag"),
        "events": summarize_events(seen),
    }


async def moli_10(client: Any, fixture: str) -> dict[str, Any]:
    session_id, _target_id, _ = await create_page_session(client, f"{fixture}/ax", timeout=10)
    eval_id = await client.send(
        "Runtime.evaluate",
        {
            "expression": "globalThis.__confirmResult = confirm('moli confirm?'); globalThis.__confirmResult",
            "returnByValue": True,
        },
        session_id=session_id,
    )
    seen: list[dict[str, Any]] = []
    eval_response: dict[str, Any] | None = None
    opening: dict[str, Any] | None = None
    deadline = asyncio.get_running_loop().time() + 8
    while opening is None:
        remaining = deadline - asyncio.get_running_loop().time()
        if remaining <= 0:
            break
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        seen.append(message)
        if message.get("id") == eval_id:
            eval_response = message
        if message.get("sessionId") == session_id and message.get("method") == "Page.javascriptDialogOpening":
            opening = message
    evaluate_pending_before_handle = eval_response is None
    handle_id = await client.send("Page.handleJavaScriptDialog", {"accept": True}, session_id=session_id)
    handle_response, handle_seen = await recv_until_id(client, handle_id, timeout=5, allow_error=True)
    if eval_response is None:
        eval_response = next(
            (message for message in handle_seen if message.get("id") == eval_id),
            None,
        )
    if eval_response is None:
        eval_response, eval_seen = await recv_until_id(client, eval_id, timeout=5, allow_error=True)
        handle_seen.extend(eval_seen)
    closed = next(
        (
            message
            for message in handle_seen
            if message.get("sessionId") == session_id and message.get("method") == "Page.javascriptDialogClosed"
        ),
        None,
    )
    probe_id = await client.send(
        "Runtime.evaluate",
        {"expression": "globalThis.__confirmResult", "returnByValue": True},
        session_id=session_id,
    )
    probe_response, _ = await recv_until_id(client, probe_id, timeout=5, allow_error=True)
    initial_value = (eval_response or {}).get("result", {}).get("result", {}).get("value")
    final_value = probe_response.get("result", {}).get("result", {}).get("value")
    accepted_ok = "error" not in handle_response
    events_ok = opening is not None and accepted_ok and closed is not None
    status = (
        "not_reproduced"
        if events_ok and evaluate_pending_before_handle and initial_value is True and final_value is True
        else "reproduced"
    )
    return {
        "status": status,
        "opening": opening,
        "initial_confirm_value": initial_value,
        "evaluate_pending_before_handle": evaluate_pending_before_handle,
        "handle_response": handle_response,
        "closed": closed,
        "final_confirm_value": final_value,
        "events_before_handle": summarize_events(seen),
    }


CaseRunner = Any


async def run_with_fresh_server(
    case_id: str,
    runner: CaseRunner,
    *,
    binary: Path,
    fixture: str,
    serve_timeout: float,
    heavy_url: str,
    command_timeout: float,
    example_url: str,
) -> dict[str, Any]:
    serve: Serve | None = None
    client: Any | None = None
    started = time.perf_counter()
    try:
        serve = await start_moli(binary, timeout=serve_timeout)
        client = await connect_raw_cdp(serve.endpoint)
        if case_id == "MOLI-06":
            result = await runner(client, example_url)
        elif case_id in {"MOLI-07-DOMSnapshot", "MOLI-07-AX"}:
            result = await runner(client, heavy_url, command_timeout)
        else:
            result = await runner(client, fixture)
        result.setdefault("status", "unknown")
        return {
            "case": case_id,
            "ok": result["status"] == "not_reproduced",
            "elapsed_ms": round((time.perf_counter() - started) * 1000.0, 1),
            "result": result,
        }
    except Exception as error:
        return {
            "case": case_id,
            "ok": False,
            "elapsed_ms": round((time.perf_counter() - started) * 1000.0, 1),
            "result": {"status": "reproduced", "error": repr(error)},
        }
    finally:
        if client is not None:
            try:
                await client.websocket.close()
            except Exception:
                pass
        stopped = await stop_moli(serve)
        if stopped and "result" in locals() and isinstance(result, dict):
            result["serve_stop"] = stopped


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Reproduce MOLI-05..10 CDP regressions against moli.")
    parser.add_argument("--moli-bin", help="path to moli binary; defaults to MOLI_BIN or target/release/moli")
    parser.add_argument("--example-url", default="https://example.com/", help="URL for MOLI-06")
    parser.add_argument("--heavy-url", default="https://phys.org/", help="heavy URL for MOLI-07")
    parser.add_argument("--serve-timeout", type=float, default=10.0)
    parser.add_argument("--command-timeout", type=float, default=30.0)
    parser.add_argument("--skip-heavy", action="store_true", help="skip MOLI-07 heavy page probes")
    parser.add_argument(
        "--only",
        action="append",
        choices=["MOLI-05", "MOLI-06", "MOLI-07-DOMSnapshot", "MOLI-07-AX", "MOLI-08", "MOLI-09", "MOLI-10"],
        help="run only the selected case; can be repeated",
    )
    parser.add_argument(
        "--output",
        default=str(REPO_ROOT / "benchmarks" / "results" / "moli-05-10-repro-latest.json"),
        help="JSON result path",
    )
    return parser.parse_args()


async def main_async() -> int:
    args = parse_args()
    binary = moli_binary(args.moli_bin)
    fixture_server = start_fixture_server()
    try:
        cases: list[tuple[str, CaseRunner]] = [
            ("MOLI-05", moli_05),
            ("MOLI-06", moli_06),
        ]
        if not args.skip_heavy:
            cases.extend(
                [
                    ("MOLI-07-DOMSnapshot", moli_07),
                    ("MOLI-07-AX", moli_07_ax),
                ]
            )
        cases.extend(
            [
                ("MOLI-08", moli_08),
                ("MOLI-09", moli_09),
                ("MOLI-10", moli_10),
            ]
        )
        if args.only:
            only = set(args.only)
            cases = [(case_id, runner) for case_id, runner in cases if case_id in only]
        results = []
        for case_id, runner in cases:
            print(f"running {case_id}...", flush=True)
            results.append(
                await run_with_fresh_server(
                    case_id,
                    runner,
                    binary=binary,
                    fixture=fixture_server.url,
                    serve_timeout=args.serve_timeout,
                    heavy_url=args.heavy_url,
                    command_timeout=args.command_timeout,
                    example_url=args.example_url,
                )
            )
        payload = {
            "binary": str(binary),
            "fixture": fixture_server.url,
            "example_url": args.example_url,
            "heavy_url": None if args.skip_heavy else args.heavy_url,
            "results": results,
        }
        output = Path(args.output)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(payload, indent=2, sort_keys=True), encoding="utf-8")
        print(json.dumps(payload, indent=2, sort_keys=True))
        return 0 if all(result["ok"] for result in results) else 1
    finally:
        fixture_server.close()


def main() -> int:
    return asyncio.run(main_async())


if __name__ == "__main__":
    raise SystemExit(main())
