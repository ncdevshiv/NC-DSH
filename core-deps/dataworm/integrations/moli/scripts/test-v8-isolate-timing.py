#!/usr/bin/env python3
"""Test V8 isolate initialization timing."""
from __future__ import annotations

import asyncio
import json
import time
from pathlib import Path
from typing import Any

from moli_benchmark.chrome_dcl import _recv_command_response, _wait_for_cdp
from moli_benchmark.config import REPO_ROOT
from moli_benchmark.target_serve import start_target_serve, stop_target_serve


async def _run_test_with_timing(
    endpoint: str,
    process: Any,
    url: str,
    timeout_seconds: float,
) -> dict[str, Any]:
    """Run a simple CDP test to trigger isolate initialization."""
    deadline = time.perf_counter() + timeout_seconds

    try:
        client = await _wait_for_cdp(endpoint, process, min(5.0, max(0.1, timeout_seconds)))
    except TimeoutError as error:
        return {"error": f"startup timeout: {error}"}

    target_id: str | None = None
    try:
        # Target.createTarget
        create_id = await client.send("Target.createTarget", {"url": "about:blank"})
        create_response, _ = await _recv_command_response(
            client, create_id, deadline=deadline, stage="Target.createTarget",
        )
        target_id = str(create_response["result"]["targetId"])

        # Target.attachToTarget
        attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
        attach_response, _ = await _recv_command_response(
            client, attach_id, deadline=deadline, stage="Target.attachToTarget",
        )
        session_id = str(attach_response["result"]["sessionId"])

        # Enable Page
        message_id = await client.send("Page.enable", session_id=session_id)
        await _recv_command_response(client, message_id, deadline=deadline, stage="Page.enable")

        # Navigate to trigger isolate creation
        navigate_id = await client.send("Page.navigate", {"url": url}, session_id=session_id)
        navigate_response, _ = await _recv_command_response(
            client, navigate_id, deadline=deadline, stage="Page.navigate",
        )

        # Wait a bit for initialization
        await asyncio.sleep(0.5)

        return {"status": "ok"}

    finally:
        if target_id is not None:
            try:
                close_id = await client.send("Target.closeTarget", {"targetId": target_id})
                await client.recv_until_id(close_id, timeout=1.0)
            except Exception:
                pass
        await client.websocket.close()


def main() -> None:
    moli_bin = REPO_ROOT / "target" / "release" / "moli"
    if not moli_bin.exists():
        print(f"Error: moli binary not found at {moli_bin}")
        print("Please run: cargo build --release")
        return

    url = "https://www.ifeng.com/"
    print(f"Testing V8 isolate timing with: {url}")
    print("Look for timing logs in stderr output below:")
    print("=" * 80)

    started = time.perf_counter()
    serve = None
    try:
        serve = start_target_serve("moli-cdp", moli_bin, 30.0)
        result = asyncio.run(_run_test_with_timing(serve.endpoint, serve.process, url, 30.0))
        stopped = stop_target_serve(serve)
        serve = None
        elapsed_ms = (time.perf_counter() - started) * 1000

        print("=" * 80)
        print(f"\nTotal elapsed: {elapsed_ms:.1f}ms")
        print(f"Result: {result}")

        # Print stderr for timing logs
        stderr = stopped.get("log_tail", [])
        if stderr:
            print("\n=== Server stderr (timing logs) ===")
            for line in stderr:
                print(line)

    except Exception as e:
        print(f"Error: {e}")
        if serve is not None:
            stop_target_serve(serve)


if __name__ == "__main__":
    main()
