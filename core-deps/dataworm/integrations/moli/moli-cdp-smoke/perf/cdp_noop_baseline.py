"""Phase A — CDP dispatch loop noop baseline.

Measures end-to-end round-trip cost of a near-noop CDP command
(`Browser.getVersion`) via a raw WebSocket client, isolating the dispatch
loop fixed overhead from any handler-side work.

Usage:
    uv run python moli-cdp-smoke/perf/cdp_noop_baseline.py \
        --runs 200 --engines moli lightpanda
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import socket
import statistics
import subprocess
import time
import urllib.request
from contextlib import closing
from pathlib import Path

import websockets  # type: ignore

REPO_ROOT = Path(__file__).resolve().parents[2]


def free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def resolve_moli_bin() -> str:
    return os.environ.get("MOLI_BIN", str(REPO_ROOT / "target/release/moli"))


async def wait_cdp_ready(port: int, timeout_s: float = 15.0) -> str:
    deadline = time.monotonic() + timeout_s
    last = None
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(
                f"http://127.0.0.1:{port}/json/version", timeout=0.5
            ) as r:
                payload = json.loads(r.read())
                ws = payload.get("webSocketDebuggerUrl")
                if ws:
                    return ws
        except Exception as exc:
            last = exc
        await asyncio.sleep(0.05)
    raise RuntimeError(f"CDP not ready :{port}: {last}")


def spawn(name: str, port: int) -> subprocess.Popen:
    env = {k: v for k, v in os.environ.items() if "PROXY" not in k.upper()}
    env["NO_PROXY"] = "127.0.0.1,localhost"
    if name == "moli":
        cmd = [resolve_moli_bin(), "serve", "--host", "127.0.0.1", "--port", str(port)]
    elif name == "lightpanda":
        cmd = [
            "lightpanda", "serve",
            "--host", "127.0.0.1", "--port", str(port),
            "--log_level", "fatal",
        ]
    else:
        raise ValueError(name)
    return subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL)


async def measure(name: str, port: int, runs: int, warmup: int) -> list[float]:
    ws_url = await wait_cdp_ready(port)
    samples: list[float] = []
    async with websockets.connect(ws_url, max_size=2**24) as ws:
        # warmup
        for i in range(warmup):
            await ws.send(json.dumps({"id": i + 1, "method": "Browser.getVersion"}))
            await ws.recv()
        # measured: serial RTT, no overlap
        for i in range(runs):
            req_id = warmup + i + 1
            payload = json.dumps({"id": req_id, "method": "Browser.getVersion"})
            t0 = time.perf_counter()
            await ws.send(payload)
            while True:
                raw = await ws.recv()
                msg = json.loads(raw)
                if msg.get("id") == req_id:
                    break
                # ignore stray events
            samples.append((time.perf_counter() - t0) * 1000.0)
    return samples


def summary(samples: list[float]) -> dict[str, float]:
    s = sorted(samples)
    n = len(s)
    return {
        "n": n,
        "min_ms": min(s),
        "p50_ms": statistics.median(s),
        "p95_ms": s[max(0, int(round(0.95 * (n - 1))))],
        "p99_ms": s[max(0, int(round(0.99 * (n - 1))))],
        "max_ms": max(s),
        "mean_ms": statistics.mean(s),
        "stdev_ms": statistics.pstdev(s) if n > 1 else 0.0,
    }


async def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=200)
    ap.add_argument("--warmup", type=int, default=20)
    ap.add_argument("--engines", nargs="*", default=["moli", "lightpanda"])
    args = ap.parse_args()

    print(f"runs={args.runs} warmup={args.warmup} (raw WS RTT, Browser.getVersion)")
    print()
    table: dict[str, dict[str, float]] = {}
    for engine in args.engines:
        port = free_port()
        proc = spawn(engine, port)
        try:
            samples = await measure(engine, port, args.runs, args.warmup)
            table[engine] = summary(samples)
            s = table[engine]
            print(f"{engine:14s} n={int(s['n']):3d} "
                  f"min={s['min_ms']:6.2f} p50={s['p50_ms']:6.2f} "
                  f"p95={s['p95_ms']:6.2f} p99={s['p99_ms']:6.2f} "
                  f"max={s['max_ms']:6.2f} mean={s['mean_ms']:6.2f} ms")
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

    if len(args.engines) == 2 and all(e in table for e in args.engines):
        a, b = args.engines
        ratio = table[a]["p50_ms"] / table[b]["p50_ms"]
        print(f"\n{a} P50 / {b} P50 = {ratio:.2f}x")


if __name__ == "__main__":
    asyncio.run(main())
