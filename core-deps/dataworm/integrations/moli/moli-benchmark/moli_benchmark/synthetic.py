from __future__ import annotations

import hashlib
import os
import socket
import subprocess
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from .artifacts import write_csv, write_json
from .config import REPO_ROOT, clear_proxy_env
from .process import run_process
from .stats import summarize
from .synthetic_cases import SYNTHETIC_CASES, response_for_path


class SyntheticHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        response = response_for_path(parsed.path)
        if response is not None:
            if response.delay_seconds:
                time.sleep(response.delay_seconds)
            self._send(response.content_type, response.body)
            return
        self.send_error(404)

    def log_message(self, format: str, *args: object) -> None:
        return

    def _send(self, content_type: str, body: bytes) -> None:
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except BrokenPipeError:
            return


class SyntheticServer:
    def __init__(self) -> None:
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), SyntheticHandler)
        self.port = int(self.httpd.server_address[1])
        self.thread = threading.Thread(target=self.httpd.serve_forever, name="moli-benchmark-fixture", daemon=True)
        self.external_host = _global_ipv6_address()
        self.external_httpd: ThreadingHTTPServer | None = None
        self.external_port: int | None = None
        self.external_thread: threading.Thread | None = None
        if self.external_host is not None:
            try:
                self.external_httpd = _SyntheticIpv6Server((self.external_host, 0), SyntheticHandler)
                self.external_port = int(self.external_httpd.server_address[1])
                self.external_thread = threading.Thread(
                    target=self.external_httpd.serve_forever,
                    name="moli-benchmark-external-fixture",
                    daemon=True,
                )
            except OSError:
                self.external_host = None
                self.external_httpd = None
                self.external_port = None
                self.external_thread = None

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    @property
    def external_base_url(self) -> str | None:
        if self.external_host is None or self.external_port is None:
            return None
        return f"http://[{self.external_host}]:{self.external_port}"

    def url_for_path(self, path: str, *, external: bool = False) -> str:
        base_url = self.external_base_url if external and self.external_base_url is not None else self.base_url
        return f"{base_url}/{path.lstrip('/')}"

    def __enter__(self) -> "SyntheticServer":
        self.thread.start()
        if self.external_thread is not None:
            self.external_thread.start()
        time.sleep(0.025)
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=2)
        if self.external_httpd is not None:
            self.external_httpd.shutdown()
            self.external_httpd.server_close()
        if self.external_thread is not None:
            self.external_thread.join(timeout=2)


class _SyntheticIpv6Server(ThreadingHTTPServer):
    address_family = socket.AF_INET6


def _global_ipv6_address() -> str | None:
    try:
        output = subprocess.check_output(
            ["ip", "-o", "-6", "addr", "show", "scope", "global"],
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=2,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    for line in output.splitlines():
        parts = line.split()
        if "inet6" not in parts:
            continue
        address = parts[parts.index("inet6") + 1].split("/", 1)[0]
        if address and not address.lower().startswith("fe80:"):
            return address
    return None


def _hash_output(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run_synthetic_suite(
    *,
    moli_bin: Path,
    output_dir: Path,
    runs: int,
    timeout_seconds: float,
    cases: tuple[str, ...],
    concurrency: int = 1,
) -> dict[str, Any]:
    unknown = [case for case in cases if case not in SYNTHETIC_CASES]
    if unknown:
        raise RuntimeError(f"unknown synthetic case(s): {', '.join(unknown)}")

    suite_dir = output_dir / "synthetic"
    rows: list[dict[str, Any]] = []
    details: list[dict[str, Any]] = []

    def run_one(server: SyntheticServer, case: str, run_id: int) -> tuple[dict[str, Any], dict[str, Any]]:
        url = f"{server.base_url}/{case}"
        result = run_process(
            [
                str(moli_bin),
                "fetch",
                "--dump",
                "html",
                "--wait-until",
                "done",
                "--wait-script",
                "document.querySelector('[data-benchmark-status=\"ok\"]') !== null",
                "--timeout",
                str(int(timeout_seconds * 1000)),
                url,
            ],
            cwd=REPO_ROOT,
            timeout_seconds=timeout_seconds + 1,
            env=clear_proxy_env(os.environ),
        )
        output = result.output_digest_material()
        ok = result.returncode == 0 and not result.timed_out and b"data-benchmark-status=\"ok\"" in result.stdout
        row = {
            "case": case,
            "run": run_id,
            "concurrency": concurrency,
            "ok": ok,
            "elapsed_ms": result.elapsed_ms,
            "returncode": result.returncode,
            "timed_out": result.timed_out,
            "peak_pss_bytes": result.resources.get("peak_pss_bytes"),
            "peak_cpu_percent": result.resources.get("peak_cpu_percent"),
            "output_sha256": _hash_output(output),
        }
        return row, {**row, "url": url, "process": result.json_summary(include_output=not ok)}

    with SyntheticServer() as server:
        for case in cases:
            with ThreadPoolExecutor(max_workers=max(1, concurrency)) as executor:
                futures = [executor.submit(run_one, server, case, run_id) for run_id in range(1, runs + 1)]
                for future in as_completed(futures):
                    row, detail = future.result()
                    rows.append(row)
                    details.append(detail)

    summary = {
        "suite": "synthetic",
        "runs": runs,
        "timeout_seconds": timeout_seconds,
        "concurrency": concurrency,
        "cases": {},
        "total_failures": sum(1 for row in rows if not row.get("ok")),
    }
    for case in cases:
        case_rows = [row for row in rows if row["case"] == case]
        summary["cases"][case] = {
            "elapsed_ms": summarize(row["elapsed_ms"] for row in case_rows if row.get("ok")),
            "peak_pss_bytes": summarize(row["peak_pss_bytes"] for row in case_rows if row.get("peak_pss_bytes") is not None),
            "failures": sum(1 for row in case_rows if not row.get("ok")),
        }

    write_csv(suite_dir / "runs.csv", rows)
    write_json(suite_dir / "runs.json", details)
    write_json(suite_dir / "summary.json", summary)
    return summary
