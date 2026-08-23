from __future__ import annotations

import asyncio
import os
import shutil
import sys
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any

from .config import REPO_ROOT, clear_proxy_env, moli_binary


@dataclass
class MoliServe:
    process: asyncio.subprocess.Process
    logs: list[str]
    tasks: list[asyncio.Task[Any]]
    http_cache_dir: str


async def _collect_process_output(stream: asyncio.StreamReader | None, logs: list[str], label: str) -> None:
    if stream is None:
        return
    while True:
        line = await stream.readline()
        if not line:
            return
        text = line.decode("utf-8", errors="replace").rstrip()
        logs.append(f"{label}: {text}")
        if os.environ.get("MOLI_SMOKE_TRACE_BG") == "1":
            print(f"[moli serve {label}] {text}", file=sys.stderr, flush=True)


async def start_moli_serve(
    port: int,
    *,
    layout: bool = True,
    extra_args: tuple[str, ...] = (),
) -> MoliServe:
    binary = moli_binary()
    http_cache_dir = tempfile.mkdtemp(prefix="moli-cdp-smoke-cache-")
    command = [
        str(binary),
        "serve",
        "--host",
        "127.0.0.1",
        "--port",
        str(port),
        "--resource",
    ]
    if layout:
        command.append("--layout")
    command.extend(("--http-cache-dir", http_cache_dir, *extra_args))
    try:
        process = await asyncio.create_subprocess_exec(
            *command,
            cwd=str(REPO_ROOT),
            env=clear_proxy_env(os.environ),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
    except Exception:
        shutil.rmtree(http_cache_dir, ignore_errors=True)
        raise
    logs: list[str] = []
    tasks = [
        asyncio.create_task(_collect_process_output(process.stdout, logs, "stdout")),
        asyncio.create_task(_collect_process_output(process.stderr, logs, "stderr")),
    ]
    return MoliServe(
        process=process,
        logs=logs,
        tasks=tasks,
        http_cache_dir=http_cache_dir,
    )


async def stop_moli_serve(serve: MoliServe | None) -> None:
    if serve is None:
        return
    if serve.process.returncode is None:
        serve.process.terminate()
        try:
            await asyncio.wait_for(serve.process.wait(), timeout=5)
        except asyncio.TimeoutError:
            serve.process.kill()
            await serve.process.wait()
    for task in serve.tasks:
        task.cancel()
    await asyncio.gather(*serve.tasks, return_exceptions=True)
    shutil.rmtree(serve.http_cache_dir, ignore_errors=True)


def _probe_url(url: str) -> bool:
    try:
        with urllib.request.urlopen(url, timeout=0.5) as response:
            response.read()
        return True
    except (urllib.error.URLError, TimeoutError, OSError):
        return False


async def wait_for_cdp_server(
    endpoint: str,
    serve: MoliServe | None,
) -> None:
    version_url = endpoint.rstrip("/") + "/json/version"
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if serve is not None and serve.process.returncode is not None:
            tail = "\n".join(serve.logs[-80:])
            raise RuntimeError(f"moli serve exited early with {serve.process.returncode}\n{tail}")
        if await asyncio.to_thread(_probe_url, version_url):
            return
        await asyncio.sleep(0.05)
    tail_text = "\n".join(serve.logs[-80:]) if serve is not None else ""
    tail = f"\n{tail_text}" if tail_text else ""
    raise RuntimeError(f"timed out waiting for CDP server at {endpoint}{tail}")
