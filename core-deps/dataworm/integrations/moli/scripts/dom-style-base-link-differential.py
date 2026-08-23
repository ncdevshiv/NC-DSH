#!/usr/bin/env python3
"""Compare dynamic <base> behavior for an already loaded linked stylesheet."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlsplit


REPO_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_ROOT = (
    REPO_ROOT
    / "moli-test-support"
    / "fixtures"
    / "dom_style"
    / "base_link_existing_sheet"
)
DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()
DEFAULT_MOLI = Path(
    os.environ.get("MOLI_BINARY", str(REPO_ROOT / "target" / "debug" / "moli"))
).expanduser()


class ProbeError(RuntimeError):
    pass


@dataclass
class ProbeState:
    requested_paths: list[str] = field(default_factory=list)
    report: dict[str, Any] | None = None
    report_event: threading.Event = field(default_factory=threading.Event)
    lock: threading.Lock = field(default_factory=threading.Lock)

    def record_path(self, path: str) -> None:
        with self.lock:
            self.requested_paths.append(path)

    def record_report(self, report: dict[str, Any]) -> None:
        with self.lock:
            if self.report is None:
                self.report = report
                self.report_event.set()


class ProbeHandler(BaseHTTPRequestHandler):
    server_version = "MoliDomStyleProbe/1.0"

    @property
    def state(self) -> ProbeState:
        return self.server.probe_state  # type: ignore[attr-defined]

    def log_message(self, format: str, *args: Any) -> None:
        return

    def send_body(self, status: int, content_type: str, body: bytes) -> None:
        self.send_response(status)
        self.send_header("content-type", content_type)
        self.send_header("cache-control", "no-store")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        parsed = urlsplit(self.path)
        path = parsed.path
        self.state.record_path(path)

        if path == "/report":
            values = parse_qs(parsed.query).get("payload", [])
            if len(values) != 1:
                self.send_body(400, "text/plain; charset=utf-8", b"missing payload")
                return
            try:
                report = json.loads(values[0])
            except (TypeError, ValueError, json.JSONDecodeError) as error:
                self.send_body(
                    400,
                    "text/plain; charset=utf-8",
                    f"invalid payload: {error}".encode(),
                )
                return
            if not isinstance(report, dict):
                self.send_body(400, "text/plain; charset=utf-8", b"payload must be an object")
                return
            self.state.record_report(report)
            self.send_body(204, "text/plain; charset=utf-8", b"")
            return

        if path == "/favicon.ico":
            self.send_body(204, "image/x-icon", b"")
            return

        routes = {
            "/": ("index.html", "text/html; charset=utf-8"),
            "/index.html": ("index.html", "text/html; charset=utf-8"),
            "/old/index.html": ("index.html", "text/html; charset=utf-8"),
            "/style.css": ("old/style.css", "text/css; charset=utf-8"),
            "/old/style.css": ("old/style.css", "text/css; charset=utf-8"),
            "/new/style.css": ("new/style.css", "text/css; charset=utf-8"),
        }
        route = routes.get(path)
        if route is None:
            self.send_body(404, "text/plain; charset=utf-8", b"not found")
            return
        relative_path, content_type = route
        self.send_body(200, content_type, (FIXTURE_ROOT / relative_path).read_bytes())


@dataclass
class FixtureServer:
    server: ThreadingHTTPServer
    thread: threading.Thread
    state: ProbeState
    url: str

    def close(self) -> None:
        self.server.shutdown()
        self.thread.join(timeout=2)
        self.server.server_close()


def start_fixture_server(document_path: str) -> FixtureServer:
    state = ProbeState()
    server = ThreadingHTTPServer(("127.0.0.1", 0), ProbeHandler)
    server.daemon_threads = True
    server.probe_state = state  # type: ignore[attr-defined]
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    host, port = server.server_address
    return FixtureServer(server, thread, state, f"http://{host}:{port}{document_path}")


def clean_environment() -> dict[str, str]:
    environment = dict(os.environ)
    for name in (
        "ALL_PROXY",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "all_proxy",
        "https_proxy",
        "http_proxy",
    ):
        environment.pop(name, None)
    environment["NO_PROXY"] = "127.0.0.1,localhost"
    environment["no_proxy"] = "127.0.0.1,localhost"
    return environment


def git_revision(path: Path) -> str | None:
    for candidate in (path, *path.parents):
        if not (candidate / ".git").exists():
            continue
        result = subprocess.run(
            ["git", "-C", str(candidate), "rev-parse", "HEAD"],
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            return result.stdout.strip()
    return None


def output_tail(stream: Any, limit: int = 20_000) -> str:
    stream.flush()
    stream.seek(0)
    return stream.read().decode("utf-8", errors="replace")[-limit:]


def terminate_process(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=3)


def wait_for_report(
    process: subprocess.Popen[bytes], state: ProbeState, timeout: float
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if state.report_event.wait(timeout=0.02):
            assert state.report is not None
            return state.report
        returncode = process.poll()
        if returncode is not None:
            raise ProbeError(f"browser exited with {returncode} before reporting")
    raise ProbeError(f"timed out after {timeout:.1f}s waiting for fixture report")


def normalized_url_path(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    return urlsplit(value).path


def normalized_css_url_path(value: Any) -> str | None:
    if not isinstance(value, str) or not value.startswith("url(") or not value.endswith(")"):
        return None
    url = value[4:-1].strip().strip('"\'')
    return normalized_url_path(url)


def normalized_report(report: dict[str, Any]) -> dict[str, Any]:
    def normalize_snapshot(value: Any) -> dict[str, Any] | None:
        if not isinstance(value, dict):
            return None
        return {
            "link_href_path": normalized_url_path(value.get("linkHref")),
            "sheet_href_path": normalized_url_path(value.get("sheetHref")),
            "color": value.get("color"),
            "inline_background_path": normalized_css_url_path(
                value.get("inlineBackground")
            ),
        }

    return {
        "status": report.get("status"),
        "same_sheet": report.get("sameSheet"),
        "before": normalize_snapshot(report.get("before")),
        "after": normalize_snapshot(report.get("after")),
    }


def matches_chromium_semantics(report: dict[str, Any], requested_paths: list[str]) -> bool:
    normalized = normalized_report(report)
    return normalized == {
        "status": "ok",
        "same_sheet": True,
        "before": {
            "link_href_path": "/old/style.css",
            "sheet_href_path": "/old/style.css",
            "color": "rgb(1, 2, 3)",
            "inline_background_path": "/old/img.png",
        },
        "after": {
            "link_href_path": "/new/style.css",
            "sheet_href_path": "/old/style.css",
            "color": "rgb(1, 2, 3)",
            "inline_background_path": "/old/img.png",
        },
    } and requested_paths.count("/old/style.css") == 1 and "/new/style.css" not in requested_paths


def matches_m1_semantics(report: dict[str, Any], requested_paths: list[str]) -> bool:
    """M1 freezes an existing sheet; M4 will make its CSSOM href absolute."""
    normalized = normalized_report(report)
    before = normalized.get("before")
    after = normalized.get("after")
    return (
        normalized.get("status") == "ok"
        and normalized.get("same_sheet") is True
        and isinstance(before, dict)
        and isinstance(after, dict)
        and before.get("link_href_path") == "/old/style.css"
        and after.get("link_href_path") == "/new/style.css"
        and before.get("color") == "rgb(1, 2, 3)"
        and after.get("color") == "rgb(1, 2, 3)"
        and before.get("inline_background_path") == "/old/img.png"
        and after.get("inline_background_path") == "/old/img.png"
        and before.get("sheet_href_path") is not None
        and before.get("sheet_href_path") == after.get("sheet_href_path")
        and requested_paths.count("/old/style.css") == 1
        and "/style.css" not in requested_paths
        and "/new/style.css" not in requested_paths
    )


def run_chromium(binary: Path, timeout: float, document_path: str) -> dict[str, Any]:
    if not binary.is_file():
        raise ProbeError(f"Chromium binary does not exist: {binary}")
    fixture = start_fixture_server(document_path)
    try:
        with tempfile.TemporaryDirectory(prefix="moli-chromium-profile-") as profile:
            with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
                process = subprocess.Popen(
                    [
                        str(binary),
                        "--headless=new",
                        "--disable-background-networking",
                        "--disable-default-apps",
                        "--disable-gpu",
                        "--no-first-run",
                        "--no-sandbox",
                        f"--user-data-dir={profile}",
                        fixture.url,
                    ],
                    cwd=REPO_ROOT,
                    env=clean_environment(),
                    stdout=stdout,
                    stderr=stderr,
                )
                try:
                    report = wait_for_report(process, fixture.state, timeout)
                finally:
                    terminate_process(process)
                return {
                    "target": "chromium",
                    "binary": str(binary),
                    "source_revision": git_revision(binary),
                    "returncode": process.returncode,
                    "report": report,
                    "normalized_report": normalized_report(report),
                    "requested_paths": fixture.state.requested_paths,
                    "matches_chromium_semantics": matches_chromium_semantics(
                        report, fixture.state.requested_paths
                    ),
                    "matches_m1_semantics": matches_m1_semantics(
                        report, fixture.state.requested_paths
                    ),
                    "stdout_tail": output_tail(stdout),
                    "stderr_tail": output_tail(stderr),
                }
    finally:
        fixture.close()


def run_moli(binary: Path, timeout: float, document_path: str) -> dict[str, Any]:
    if not binary.is_file():
        raise ProbeError(f"Moli binary does not exist: {binary}")
    fixture = start_fixture_server(document_path)
    try:
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            process = subprocess.Popen(
                [
                    str(binary),
                    "fetch",
                    "--dump",
                    "json",
                    "--trace-network",
                    "--wait-until",
                    "domcontentloaded",
                    "--wait-script",
                    "document.documentElement.dataset.probeDone === 'true'",
                    "--timeout",
                    str(round(timeout * 1000)),
                    fixture.url,
                ],
                cwd=REPO_ROOT,
                env=clean_environment(),
                stdout=stdout,
                stderr=stderr,
            )
            try:
                report = wait_for_report(process, fixture.state, timeout)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    terminate_process(process)
            finally:
                terminate_process(process)
            return {
                "target": "moli",
                "binary": str(binary),
                "returncode": process.returncode,
                "report": report,
                "normalized_report": normalized_report(report),
                "requested_paths": fixture.state.requested_paths,
                "matches_chromium_semantics": matches_chromium_semantics(
                    report, fixture.state.requested_paths
                ),
                "matches_m1_semantics": matches_m1_semantics(
                    report, fixture.state.requested_paths
                ),
                "stdout_tail": output_tail(stdout),
                "stderr_tail": output_tail(stderr),
            }
    finally:
        fixture.close()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--target",
        choices=("all", "chromium", "moli"),
        default="all",
        help="browser target to run",
    )
    parser.add_argument("--chromium", type=Path, default=DEFAULT_CHROMIUM)
    parser.add_argument("--moli", type=Path, default=DEFAULT_MOLI)
    parser.add_argument("--timeout", type=float, default=15.0)
    parser.add_argument(
        "--document-path",
        choices=("/old/index.html", "/index.html"),
        default="/old/index.html",
        help=(
            "use /old/index.html to isolate dynamic base changes, or /index.html "
            "to verify that initial relative link processing uses <base>"
        ),
    )
    parser.add_argument(
        "--require-match",
        choices=("none", "all", "chromium", "moli"),
        default="none",
        help="fail unless the selected target(s) match --contract",
    )
    parser.add_argument(
        "--contract",
        choices=("chromium", "m1"),
        default="chromium",
        help=(
            "semantic contract used by --require-match; M1 permits the known "
            "pre-M4 CSSStyleSheet.href representation"
        ),
    )
    parser.add_argument("--output", type=Path, help="also write the JSON report to this path")
    parser.add_argument(
        "--include-process-output",
        action="store_true",
        help="include browser stdout/stderr tails in the JSON report",
    )
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    return args


def main() -> int:
    args = parse_args()
    results: list[dict[str, Any]] = []
    try:
        if args.target in ("all", "chromium"):
            results.append(
                run_chromium(args.chromium.expanduser(), args.timeout, args.document_path)
            )
        if args.target in ("all", "moli"):
            results.append(
                run_moli(args.moli.expanduser(), args.timeout, args.document_path)
            )
    except ProbeError as error:
        print(json.dumps({"error": str(error)}, indent=2), flush=True)
        return 2

    payload = {
        "fixture": str(FIXTURE_ROOT.relative_to(REPO_ROOT)),
        "document_path": args.document_path,
        "contract": args.contract,
        "repository_revision": git_revision(REPO_ROOT),
        "results": results,
    }
    if not args.include_process_output:
        for result in results:
            result.pop("stdout_tail", None)
            result.pop("stderr_tail", None)
    rendered = json.dumps(payload, indent=2, sort_keys=True)
    print(rendered)
    if args.output is not None:
        args.output.write_text(rendered + "\n", encoding="utf-8")

    required_targets = {
        "none": set(),
        "all": {"chromium", "moli"},
        "chromium": {"chromium"},
        "moli": {"moli"},
    }[args.require_match]
    mismatches = [
        result["target"]
        for result in results
        if result["target"] in required_targets
        and not result[f"matches_{args.contract}_semantics"]
    ]
    if mismatches:
        print(f"semantic mismatch: {', '.join(mismatches)}", flush=True)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
