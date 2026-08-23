from __future__ import annotations

import asyncio
import json
import os
import sys
from pathlib import Path
from typing import Any

from ..assertions import SmokeError
from ..config import clear_proxy_env


PUPPETEER_SCRIPT = Path(__file__).resolve().parents[1] / "puppeteer_smoke.mjs"


async def run_puppeteer_group(endpoint: str, fixture: str, results: list[dict[str, Any]]) -> None:
    node = os.environ.get("NODE", "node")
    process = await asyncio.create_subprocess_exec(
        node,
        str(PUPPETEER_SCRIPT),
        endpoint,
        fixture,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=clear_proxy_env(os.environ),
    )
    try:
        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=30)
    except asyncio.TimeoutError as error:
        process.kill()
        stdout, stderr = await process.communicate()
        stdout_text = stdout.decode("utf-8", errors="replace")
        stderr_text = stderr.decode("utf-8", errors="replace")
        raise SmokeError(
            f"Puppeteer smoke timed out after 30s\nstdout:\n{stdout_text}\nstderr:\n{stderr_text}"
        ) from error

    stdout_text = stdout.decode("utf-8", errors="replace")
    stderr_text = stderr.decode("utf-8", errors="replace")
    if process.returncode != 0:
        raise SmokeError(
            "Puppeteer smoke failed with exit code "
            f"{process.returncode}\nstdout:\n{stdout_text}\nstderr:\n{stderr_text}"
        )

    try:
        payload = json.loads(stdout_text)
    except json.JSONDecodeError as error:
        raise SmokeError(
            f"Puppeteer smoke did not return JSON\nstdout:\n{stdout_text}\nstderr:\n{stderr_text}"
        ) from error

    if not payload.get("ok"):
        raise SmokeError(f"Puppeteer smoke reported failure: {payload}")
    payload_results = payload.get("results")
    if not isinstance(payload_results, list):
        raise SmokeError(f"Puppeteer smoke returned invalid results: {payload}")
    results.extend(payload_results)
    if os.environ.get("MOLI_SMOKE_TRACE") == "1" and stderr_text:
        print(stderr_text, file=sys.stderr, end="")
