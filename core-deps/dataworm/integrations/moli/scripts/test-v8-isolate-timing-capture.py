#!/usr/bin/env python3
"""Test V8 isolate initialization timing - capture ALL stderr."""
from __future__ import annotations

import asyncio
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from moli_benchmark.chrome_dcl import _recv_command_response, _wait_for_cdp
from moli_benchmark.config import REPO_ROOT, clear_proxy_env, reserve_port


def main() -> None:
    moli_bin = REPO_ROOT / "target" / "release" / "moli"
    if not moli_bin.exists():
        print(f"Error: moli binary not found at {moli_bin}")
        return

    reserved_port = reserve_port()
    port = reserved_port.port
    stderr_file = tempfile.NamedTemporaryFile(mode='w+', suffix='.log', delete=False)
    stderr_path = stderr_file.name

    print(f"Starting moli serve on port {port}")
    print(f"Stderr captured to: {stderr_path}")

    env = clear_proxy_env(os.environ)
    env["MOLI_CDP_NAV_TIMING"] = "1"
    env["RUST_LOG"] = "moli_cdp_nav_timing=info"

    proc = subprocess.Popen(
        [str(moli_bin), "serve", "--port", str(port)],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=stderr_file,
    )

    # Wait for serve to be ready
    time.sleep(2)

    print("Triggering CDP navigate to ifeng...")
    try:
        result = asyncio.run(_trigger_navigate(port, "https://www.ifeng.com/"))
        print(f"Navigate result: {result}")
    except Exception as e:
        print(f"Navigate error: {e}")

    # Wait for logs to flush
    time.sleep(2)

    # Terminate
    proc.terminate()
    proc.wait(timeout=10)
    stderr_file.close()

    # Read and filter stderr
    print("\n=== V8 Isolate Timing Logs ===")
    with open(stderr_path) as f:
        for line in f:
            if any(kw in line for kw in [
                "v8_isolate", "isolate_bootstrap", "context_bootstrap",
                "constructor_specs", "build_constructor", "native_bridge",
                "inspector_backend", "define_global", "setup_inheritance"
            ]):
                print(line.rstrip())

    # Clean up
    os.unlink(stderr_path)


async def _trigger_navigate(port: int, url: str) -> dict:
    from moli_benchmark.raw_cdp import connect_raw_cdp
    import urllib.request

    # Get the WebSocket endpoint
    version_url = f"http://127.0.0.1:{port}/json/version"
    version_info = json.loads(urllib.request.urlopen(version_url).read())
    ws_endpoint = version_info["webSocketDebuggerUrl"]

    client = await connect_raw_cdp(ws_endpoint)

    try:
        create_id = await client.send("Target.createTarget", {"url": "about:blank"})
        create_response, _ = await client.recv_until_id(create_id, timeout=5.0)
        target_id = str(create_response["result"]["targetId"])

        attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
        attach_response, _ = await client.recv_until_id(attach_id, timeout=5.0)
        session_id = str(attach_response["result"]["sessionId"])

        for method in ("Page.enable", "Runtime.enable", "Network.enable"):
            mid = await client.send(method, session_id=session_id)
            await client.recv_until_id(mid, timeout=5.0)

        mid = await client.send("Page.setLifecycleEventsEnabled", {"enabled": True}, session_id=session_id)
        await client.recv_until_id(mid, timeout=5.0)

        navigate_id = await client.send("Page.navigate", {"url": url}, session_id=session_id)
        navigate_response, _ = await client.recv_until_id(navigate_id, timeout=30.0)

        # Wait for DCL
        deadline = time.time() + 15
        while time.time() < deadline:
            try:
                msg = await asyncio.wait_for(client.recv(), timeout=1.0)
                if msg.get("method") == "Page.domContentEventFired":
                    break
            except asyncio.TimeoutError:
                break

        return {"status": "ok", "target_id": target_id}
    finally:
        await client.websocket.close()


if __name__ == "__main__":
    main()
