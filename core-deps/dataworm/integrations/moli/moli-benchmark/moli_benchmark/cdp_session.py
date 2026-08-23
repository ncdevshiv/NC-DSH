from __future__ import annotations

import asyncio
import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .artifacts import write_csv, write_json
from .raw_cdp import RawCdpClient, connect_raw_cdp
from .stats import summarize
from .synthetic import SYNTHETIC_CASES, SyntheticServer
from .synthetic_compare import TARGETS, normalize_cdp_target, target_metadata, target_uses_external_fixture
from .target_serve import start_target_serve, stop_target_serve


@dataclass
class PageSession:
    session_id: str
    browser_context_id: str | None


TRACE_METHODS = {
    "Runtime.consoleAPICalled",
    "Runtime.exceptionThrown",
    "Log.entryAdded",
    "Network.requestWillBeSent",
    "Network.responseReceived",
    "Network.loadingFailed",
}


def _compact_remote_object(value: dict[str, Any]) -> Any:
    if "value" in value:
        return value.get("value")
    if "description" in value:
        return value.get("description")
    return value.get("type")


def _cdp_trace_events(messages: list[dict[str, Any]]) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    for message in messages:
        method = message.get("method")
        if method not in TRACE_METHODS:
            continue
        params = message.get("params", {})
        if not isinstance(params, dict):
            params = {}
        event: dict[str, Any] = {"method": method}
        if method == "Runtime.consoleAPICalled":
            event.update(
                {
                    "type": params.get("type"),
                    "text": " ".join(
                        str(_compact_remote_object(arg))
                        for arg in params.get("args", [])
                        if isinstance(arg, dict)
                    ),
                    "timestamp": params.get("timestamp"),
                }
            )
        elif method == "Runtime.exceptionThrown":
            details = params.get("exceptionDetails", {})
            if not isinstance(details, dict):
                details = {}
            event.update(
                {
                    "text": details.get("text"),
                    "url": details.get("url"),
                    "line_number": details.get("lineNumber"),
                    "column_number": details.get("columnNumber"),
                }
            )
        elif method == "Log.entryAdded":
            entry = params.get("entry", {})
            if not isinstance(entry, dict):
                entry = {}
            event.update(
                {
                    "level": entry.get("level"),
                    "source": entry.get("source"),
                    "text": entry.get("text"),
                    "url": entry.get("url"),
                }
            )
        elif method == "Network.requestWillBeSent":
            request = params.get("request", {})
            if not isinstance(request, dict):
                request = {}
            event.update(
                {
                    "request_id": params.get("requestId"),
                    "url": request.get("url"),
                    "request_method": request.get("method"),
                    "type": params.get("type"),
                }
            )
        elif method == "Network.responseReceived":
            response = params.get("response", {})
            if not isinstance(response, dict):
                response = {}
            event.update(
                {
                    "request_id": params.get("requestId"),
                    "url": response.get("url"),
                    "status": response.get("status"),
                    "mime_type": response.get("mimeType"),
                    "type": params.get("type"),
                }
            )
        elif method == "Network.loadingFailed":
            event.update(
                {
                    "request_id": params.get("requestId"),
                    "type": params.get("type"),
                    "error_text": params.get("errorText"),
                    "canceled": params.get("canceled"),
                }
            )
        events.append(event)
    return events


def _trace_summary(events: list[dict[str, Any]]) -> dict[str, int]:
    console_errors = sum(
        1
        for event in events
        if event.get("method") == "Runtime.consoleAPICalled" and event.get("type") in {"error", "assert"}
    )
    log_errors = sum(
        1
        for event in events
        if event.get("method") == "Log.entryAdded" and event.get("level") == "error"
    )
    return {
        "console_errors": console_errors + log_errors,
        "js_exceptions": sum(1 for event in events if event.get("method") == "Runtime.exceptionThrown"),
        "network_failures": sum(1 for event in events if event.get("method") == "Network.loadingFailed"),
    }


def _benchmark_marker_expression(*, url: str, case: str) -> str:
    expected_url = json.dumps(url)
    expected_case = json.dumps(case)
    return f"""
    (function() {{
      const node = document.querySelector('[data-benchmark-case]');
      return document.readyState === 'complete'
        && location.href === {expected_url}
        && node !== null
        && node.getAttribute('data-benchmark-case') === {expected_case}
        && document.querySelector('[data-benchmark-status="ok"]') !== null;
    }})()
    """


def _benchmark_marker_wait_expression(*, url: str, case: str, timeout_seconds: float) -> str:
    marker_expression = _benchmark_marker_expression(url=url, case=case)
    return f"""
    new Promise(resolve => {{
      const deadline = Date.now() + {int(timeout_seconds * 1000)};
      function tick() {{
        if ({marker_expression}) {{
          resolve(true);
        }} else if (Date.now() > deadline) {{
          resolve(false);
        }} else {{
          setTimeout(tick, 10);
        }}
      }}
      tick();
    }})
    """


def _write_trace_artifact(*, suite_dir: Path, row: dict[str, Any], events: list[dict[str, Any]]) -> str:
    trace_dir = suite_dir / "traces"
    name = f"{row['target']}-run-{row['run']}-{row['case']}.json"
    path = trace_dir / name
    write_json(path, {"row": row, "events": events, "summary": _trace_summary(events)})
    return str(path.relative_to(suite_dir))


async def _create_page_session(client: RawCdpClient) -> PageSession:
    browser_context_id = None
    try:
        context_id = await client.send("Target.createBrowserContext")
        context, _ = await client.recv_until_id(context_id)
        value = context.get("result", {}).get("browserContextId")
        if isinstance(value, str) and value:
            browser_context_id = value
    except Exception:
        browser_context_id = None

    params: dict[str, Any] = {"url": "about:blank"}
    if browser_context_id:
        params["browserContextId"] = browser_context_id
    target_command_id = await client.send("Target.createTarget", params)
    target_response, _ = await client.recv_until_id(target_command_id)
    target_id = target_response.get("result", {}).get("targetId")
    if not isinstance(target_id, str) or not target_id:
        raise RuntimeError(f"missing targetId in {target_response}")

    attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
    attach_response, _ = await client.recv_until_id(attach_id)
    session_id = attach_response.get("result", {}).get("sessionId")
    if not isinstance(session_id, str) or not session_id:
        raise RuntimeError(f"missing sessionId in {attach_response}")

    for method in ("Runtime.enable", "Page.enable"):
        command_id = await client.send(method, session_id=session_id)
        await client.recv_until_id(command_id)
    for method in ("Network.enable", "Log.enable"):
        try:
            command_id = await client.send(method, session_id=session_id)
            await client.recv_until_id(command_id, timeout=3)
        except Exception:
            pass
    return PageSession(session_id=session_id, browser_context_id=browser_context_id)


async def _navigate_and_wait_marker(
    client: RawCdpClient,
    session_id: str,
    url: str,
    case: str,
    timeout_seconds: float,
    target: str,
) -> tuple[bool, float, int, list[dict[str, Any]]]:
    started = time.perf_counter()
    navigate_id = await client.send("Page.navigate", {"url": url}, session_id=session_id)
    _, navigate_seen = await client.recv_until_id(navigate_id, timeout=timeout_seconds)
    if target_metadata(target)["engine"] == "obscura":
        seen = list(navigate_seen)
        deadline = time.perf_counter() + timeout_seconds
        ok = False
        while time.perf_counter() < deadline:
            evaluate_id = await client.send(
                "Runtime.evaluate",
                {
                    "expression": _benchmark_marker_expression(url=url, case=case),
                    "returnByValue": True,
                },
                session_id=session_id,
            )
            response, evaluate_seen = await client.recv_until_id(evaluate_id, timeout=3)
            seen.extend(evaluate_seen)
            if response.get("result", {}).get("result", {}).get("value") is True:
                ok = True
                break
            await asyncio.sleep(0.01)
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        return ok, elapsed_ms, len(seen), _cdp_trace_events(seen)

    evaluate_id = await client.send(
        "Runtime.evaluate",
        {"expression": _benchmark_marker_wait_expression(url=url, case=case, timeout_seconds=timeout_seconds), "awaitPromise": True, "returnByValue": True},
        session_id=session_id,
    )
    response, evaluate_seen = await client.recv_until_id(evaluate_id, timeout=timeout_seconds + 1)
    elapsed_ms = (time.perf_counter() - started) * 1000.0
    ok = response.get("result", {}).get("result", {}).get("value") is True
    seen = navigate_seen + evaluate_seen
    return ok, elapsed_ms, len(seen), _cdp_trace_events(seen)


async def _run_target_session(
    *,
    suite_dir: Path,
    target: str,
    binary: Path,
    base_url: SyntheticServer,
    cases: tuple[str, ...],
    runs: int,
    timeout_seconds: float,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    serve = None
    rows: list[dict[str, Any]] = []
    if target_uses_external_fixture(target) and base_url.external_base_url is None:
        for run_id in range(1, runs + 1):
            for case in cases:
                rows.append(
                    {
                        "target": target,
                        **target_metadata(target),
                        "case": case,
                        "run": run_id,
                        "ok": False,
                        "elapsed_ms": None,
                        "messages": 0,
                        "trace_events": 0,
                        "console_errors": 0,
                        "js_exceptions": 0,
                        "network_failures": 0,
                        "error": "external fixture address unavailable for Obscura",
                    }
                )
        return rows, {}
    try:
        serve = start_target_serve(target, binary, timeout_seconds)
        client = await connect_raw_cdp(serve.endpoint)
        try:
            page = await _create_page_session(client)
            for run_id in range(1, runs + 1):
                for case in cases:
                    ok, elapsed_ms, messages, trace_events = await _navigate_and_wait_marker(
                        client,
                        page.session_id,
                        base_url.url_for_path(case, external=target_uses_external_fixture(target)),
                        case,
                        timeout_seconds,
                        target,
                    )
                    trace_counts = _trace_summary(trace_events)
                    row = {
                        "target": target,
                        **target_metadata(target),
                        "case": case,
                        "run": run_id,
                        "ok": ok,
                        "elapsed_ms": elapsed_ms,
                        "messages": messages,
                        "trace_events": len(trace_events),
                        "console_errors": trace_counts["console_errors"],
                        "js_exceptions": trace_counts["js_exceptions"],
                        "network_failures": trace_counts["network_failures"],
                        "error": None if ok else "marker did not become true",
                    }
                    if not ok or any(trace_counts.values()):
                        row["trace_artifact"] = _write_trace_artifact(
                            suite_dir=suite_dir,
                            row=row,
                            events=trace_events,
                        )
                    rows.append(row)
            if page.browser_context_id:
                try:
                    dispose_id = await client.send("Target.disposeBrowserContext", {"browserContextId": page.browser_context_id})
                    await client.recv_until_id(dispose_id, timeout=3)
                except Exception:
                    pass
        finally:
            await client.websocket.close()
    except Exception as error:
        for run_id in range(1, runs + 1):
            for case in cases:
                rows.append(
                    {
                        "target": target,
                        **target_metadata(target),
                        "case": case,
                        "run": run_id,
                        "ok": False,
                        "elapsed_ms": None,
                        "messages": 0,
                        "trace_events": 0,
                        "console_errors": 0,
                        "js_exceptions": 0,
                        "network_failures": 0,
                        "error": str(error),
                    }
                )
    finally:
        serve_details = stop_target_serve(serve)
    return rows, serve_details


def run_cdp_session_suite(
    *,
    output_dir: Path,
    target_matrix: dict[str, Any],
    targets: tuple[str, ...],
    cases: tuple[str, ...],
    runs: int,
    timeout_seconds: float,
    gate_target: str,
) -> dict[str, Any]:
    targets = tuple(dict.fromkeys(normalize_cdp_target(target) for target in targets))
    gate_target = normalize_cdp_target(gate_target)
    unknown_targets = [target for target in targets if target not in TARGETS]
    if unknown_targets:
        raise RuntimeError(f"unknown target(s): {', '.join(unknown_targets)}")
    if gate_target not in targets:
        raise RuntimeError(f"gate target `{gate_target}` must be included in selected targets")
    unknown_cases = [case for case in cases if case not in SYNTHETIC_CASES]
    if unknown_cases:
        raise RuntimeError(f"unknown synthetic case(s): {', '.join(unknown_cases)}")

    suite_dir = output_dir / "cdp-session"
    rows: list[dict[str, Any]] = []
    serve_details: dict[str, Any] = {}
    with SyntheticServer() as server:
        for target in targets:
            metadata = target_metadata(target)
            info = target_matrix.get(metadata["binary_key"], {})
            path = info.get("path")
            if not info.get("available") or not path:
                rows.extend(
                    {
                        "target": target,
                        **metadata,
                        "case": case,
                        "run": run_id,
                        "ok": False,
                        "elapsed_ms": None,
                        "messages": 0,
                        "trace_events": 0,
                        "console_errors": 0,
                        "js_exceptions": 0,
                        "network_failures": 0,
                        "error": "target binary unavailable",
                    }
                    for run_id in range(1, runs + 1)
                    for case in cases
                )
                continue
            target_rows, target_serve = asyncio.run(
                _run_target_session(
                    suite_dir=suite_dir,
                    target=target,
                    binary=Path(path),
                    base_url=server,
                    cases=cases,
                    runs=runs,
                    timeout_seconds=timeout_seconds,
                )
            )
            rows.extend(target_rows)
            serve_details[target] = target_serve

    gate_failures = sum(1 for row in rows if row["target"] == gate_target and not row.get("ok"))
    summary: dict[str, Any] = {
        "suite": "cdp-session",
        "runs": runs,
        "timeout_seconds": timeout_seconds,
        "gate_target": gate_target,
        "gate_failures": gate_failures,
        "cases": list(cases),
        "targets": {},
        "total_failures": sum(1 for row in rows if not row.get("ok")),
        "total_trace_events": sum(1 for row in rows if row.get("trace_artifact")),
    }
    for target in targets:
        target_rows = [row for row in rows if row["target"] == target]
        target_summary = {
            **target_metadata(target),
            "failures": sum(1 for row in target_rows if not row.get("ok")),
            "cases": {},
        }
        for case in cases:
            case_rows = [row for row in target_rows if row["case"] == case]
            target_summary["cases"][case] = {
                "elapsed_ms": summarize(row["elapsed_ms"] for row in case_rows if row.get("ok") and row.get("elapsed_ms") is not None),
                "failures": sum(1 for row in case_rows if not row.get("ok")),
                "console_errors": sum(int(row.get("console_errors", 0) or 0) for row in case_rows),
                "js_exceptions": sum(int(row.get("js_exceptions", 0) or 0) for row in case_rows),
                "network_failures": sum(int(row.get("network_failures", 0) or 0) for row in case_rows),
            }
        target_summary["console_errors"] = sum(int(row.get("console_errors", 0) or 0) for row in target_rows)
        target_summary["js_exceptions"] = sum(int(row.get("js_exceptions", 0) or 0) for row in target_rows)
        target_summary["network_failures"] = sum(int(row.get("network_failures", 0) or 0) for row in target_rows)
        summary["targets"][target] = target_summary

    write_csv(suite_dir / "runs.csv", rows)
    write_json(suite_dir / "runs.json", rows)
    write_json(suite_dir / "serve.json", serve_details)
    write_json(suite_dir / "summary.json", summary)
    return summary
