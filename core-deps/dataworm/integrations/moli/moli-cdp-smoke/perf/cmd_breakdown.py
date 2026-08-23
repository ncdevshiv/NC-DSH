"""Phase B helper — Playwright command sequence + per-command service time.

Starts moli with MOLI_CMD_PROBE=1 (logs "CMD_PROBE method=... dispatch_us=..."
to stderr per command), drives a real Playwright connect_over_cdp + new_page +
goto + wait_for_function flow against a local fixture, then aggregates the
per-method service-time histogram.

Output: top methods by total time, count, P50/P95/max per method.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import re
import socket
import statistics
import subprocess
import sys
import time
from contextlib import closing
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "moli-benchmark"))
from moli_benchmark.synthetic_case_groups.basic import (  # type: ignore
    response_for_basic_path,
)

from playwright.async_api import async_playwright  # type: ignore


PROBE_RE = re.compile(r"CMD_PROBE method=(?P<method>\S+) dispatch_us=(?P<us>\d+)")


def free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


async def fixture_handle(reader: asyncio.StreamReader,
                         writer: asyncio.StreamWriter) -> None:
    try:
        line = await reader.readline()
        if not line:
            return
        try:
            _m, path, _v = line.decode().split()
        except ValueError:
            return
        while True:
            h = await reader.readline()
            if not h or h in (b"\r\n", b"\n"):
                break
        resp = response_for_basic_path(path)
        if resp is None:
            body = b"not found"
            ctype = "text/plain"
            status = "404 Not Found"
        else:
            ctype, body, _ = resp
            status = "200 OK"
        out = (
            f"HTTP/1.1 {status}\r\ncontent-type: {ctype}\r\n"
            f"content-length: {len(body)}\r\nconnection: close\r\n\r\n"
        ).encode() + body
        writer.write(out)
        await writer.drain()
    finally:
        try:
            writer.close()
        except Exception:
            pass


def terminate_process(proc: subprocess.Popen) -> None:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()


def default_moli_bin() -> str:
    return os.environ.get("MOLI_BIN", str(REPO_ROOT / "target/release/moli"))


def spawn_moli(port: int, bin_path: str, log_path: Path) -> tuple[subprocess.Popen, Path]:
    env = {k: v for k, v in os.environ.items() if "PROXY" not in k.upper()}
    env["NO_PROXY"] = "127.0.0.1,localhost"
    env["MOLI_CMD_PROBE"] = "1"
    log = log_path.open("wb")
    proc = subprocess.Popen(
        [bin_path, "serve", "--host", "127.0.0.1", "--port", str(port)],
        env=env, stdout=subprocess.DEVNULL, stderr=log,
    )
    return proc, log_path


async def wait_ready(port: int, timeout_s: float = 15.0) -> None:
    import urllib.request
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/json/version",
                                        timeout=0.5) as r:
                if r.status == 200:
                    return
        except Exception:
            pass
        await asyncio.sleep(0.05)
    raise RuntimeError(f"not ready :{port}")


async def drive_playwright(cdp_port: int, fixture_port: int, case: str,
                           runs: int) -> None:
    async with async_playwright() as pw:
        browser = await pw.chromium.connect_over_cdp(f"http://127.0.0.1:{cdp_port}")
        try:
            ctx = browser.contexts[0] if browser.contexts else await browser.new_context()
            for _ in range(runs):
                page = await ctx.new_page()
                try:
                    await page.goto(f"http://127.0.0.1:{fixture_port}/{case}",
                                    wait_until="load", timeout=20000)
                    await page.wait_for_function(
                        'document.querySelector("[data-benchmark-status=\\"ok\\"]") !== null',
                        timeout=20000,
                    )
                finally:
                    await page.close()
        finally:
            await browser.close()


def parse_probes(log_path: Path) -> dict[str, list[int]]:
    by_method: dict[str, list[int]] = {}
    if not log_path.exists():
        return by_method
    for line in log_path.read_text(errors="replace").splitlines():
        m = PROBE_RE.search(line)
        if not m:
            continue
        by_method.setdefault(m["method"], []).append(int(m["us"]))
    return by_method


def report(by_method: dict[str, list[int]]) -> None:
    rows = []
    for method, samples in by_method.items():
        s = sorted(samples)
        n = len(s)
        rows.append({
            "method": method,
            "n": n,
            "total_ms": sum(s) / 1000.0,
            "p50_us": statistics.median(s),
            "p95_us": s[max(0, int(round(0.95 * (n - 1))))],
            "max_us": max(s),
        })
    rows.sort(key=lambda r: r["total_ms"], reverse=True)
    grand = sum(r["total_ms"] for r in rows)
    print(f"\n{'method':46s} {'n':>5s} {'total_ms':>10s} {'p50_us':>8s} "
          f"{'p95_us':>8s} {'max_us':>8s} {'%':>5s}")
    print("-" * 100)
    for r in rows:
        pct = 100.0 * r["total_ms"] / grand if grand else 0
        print(f"{r['method']:46s} {r['n']:5d} {r['total_ms']:10.2f} "
              f"{r['p50_us']:8.0f} {r['p95_us']:8.0f} {r['max_us']:8.0f} "
              f"{pct:5.1f}")
    print(f"\nTotal commands: {sum(r['n'] for r in rows)}, "
          f"total dispatch time: {grand:.2f} ms")


async def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--case", default="static-html")
    ap.add_argument("--runs", type=int, default=8)
    ap.add_argument("--moli-bin", default=default_moli_bin())
    ap.add_argument("--probe-log", type=Path, default=None)
    args = ap.parse_args()

    fixture_port = free_port()
    fixture = await asyncio.start_server(fixture_handle, "127.0.0.1", fixture_port)
    cdp_port = free_port()
    log_path = args.probe_log or Path(f"/tmp/moli_probe_{os.getpid()}_{cdp_port}.stderr")
    proc, log_path = spawn_moli(cdp_port, args.moli_bin, log_path)
    try:
        await wait_ready(cdp_port)
        # warmup once (creates first about:blank page etc.)
        await drive_playwright(cdp_port, fixture_port, args.case, runs=1)
        # Restart moli so measured runs are written to a fresh stderr
        # file descriptor. Truncating an open child stderr file is unreliable.
        terminate_process(proc)
        proc, log_path = spawn_moli(cdp_port, args.moli_bin, log_path)
        await wait_ready(cdp_port)
        await drive_playwright(cdp_port, fixture_port, args.case, runs=args.runs)
    finally:
        fixture.close()
        await fixture.wait_closed()
        terminate_process(proc)

    by_method = parse_probes(log_path)
    print(f"case={args.case} runs={args.runs}")
    report(by_method)


if __name__ == "__main__":
    asyncio.run(main())
