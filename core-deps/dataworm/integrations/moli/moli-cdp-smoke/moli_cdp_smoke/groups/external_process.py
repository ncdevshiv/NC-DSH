from __future__ import annotations

import asyncio
import json
import os
import sys
from collections.abc import Sequence
from typing import Any

from ..assertions import SmokeError
from ..config import clear_proxy_env


async def run_external_json_process(
    label: str,
    argv: Sequence[str],
    results: list[dict[str, Any]],
    *,
    timeout_seconds: float = 45,
) -> None:
    process = await asyncio.create_subprocess_exec(
        *argv,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=clear_proxy_env(os.environ),
    )
    try:
        stdout, stderr = await asyncio.wait_for(
            process.communicate(), timeout=timeout_seconds
        )
    except asyncio.TimeoutError as error:
        process.kill()
        stdout, stderr = await process.communicate()
        raise SmokeError(
            f"{label} smoke timed out after {timeout_seconds:g}s\n"
            f"stdout:\n{stdout.decode(errors='replace')}\n"
            f"stderr:\n{stderr.decode(errors='replace')}"
        ) from error

    stdout_text = stdout.decode("utf-8", errors="replace")
    stderr_text = stderr.decode("utf-8", errors="replace")
    if process.returncode != 0:
        raise SmokeError(
            f"{label} smoke failed with exit code {process.returncode}\n"
            f"stdout:\n{stdout_text}\nstderr:\n{stderr_text}"
        )

    try:
        payload = json.loads(stdout_text)
    except json.JSONDecodeError as error:
        raise SmokeError(
            f"{label} smoke did not return JSON\n"
            f"stdout:\n{stdout_text}\nstderr:\n{stderr_text}"
        ) from error

    if not isinstance(payload, dict) or payload.get("ok") is not True:
        raise SmokeError(f"{label} smoke reported failure: {payload!r}")
    payload_results = payload.get("results")
    if not isinstance(payload_results, list) or not all(
        isinstance(result, dict) for result in payload_results
    ):
        raise SmokeError(f"{label} smoke returned invalid results: {payload!r}")
    results.extend(payload_results)
    if os.environ.get("MOLI_SMOKE_TRACE") == "1" and stderr_text:
        print(stderr_text, file=sys.stderr, end="")
