#!/usr/bin/env python3
"""Test V8 isolate initialization timing - capture ALL logs."""
from __future__ import annotations

import asyncio
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path

from moli_benchmark.config import REPO_ROOT, clear_proxy_env, reserve_port


def main() -> None:
    moli_bin = REPO_ROOT / "target" / "release" / "moli"
    if not moli_bin.exists():
        print(f"Error: moli binary not found at {moli_bin}")
        return

    reserved_port = reserve_port()
    port = reserved_port.port

    # Create a temp file for stderr
    stderr_fd, stderr_path = tempfile.mkstemp(suffix='.log')
    stderr_file = os.fdopen(stderr_fd, 'w')

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
    time.sleep(3)

    # Close stderr file so we can read it
    stderr_file.close()

    # Trigger navigate using subprocess
    print("Triggering CDP navigate to ifeng...")
    navigate_script = f"""
import asyncio, json, time, urllib.request
from moli_benchmark.raw_cdp import connect_raw_cdp

async def main():
    version_info = json.loads(urllib.request.urlopen('http://127.0.0.1:{port}/json/version').read())
    ws_endpoint = version_info['webSocketDebuggerUrl']
    client = await connect_raw_cdp(ws_endpoint)

    create_id = await client.send('Target.createTarget', {{'url': 'about:blank'}})
    create_response, _ = await client.recv_until_id(create_id, timeout=5.0)
    target_id = str(create_response['result']['targetId'])

    attach_id = await client.send('Target.attachToTarget', {{'targetId': target_id, 'flatten': True}})
    attach_response, _ = await client.recv_until_id(attach_id, timeout=5.0)
    session_id = str(attach_response['result']['sessionId'])

    for method in ('Page.enable', 'Runtime.enable'):
        mid = await client.send(method, session_id=session_id)
        await client.recv_until_id(mid, timeout=5.0)

    mid = await client.send('Page.setLifecycleEventsEnabled', {{'enabled': True}}, session_id=session_id)
    await client.recv_until_id(mid, timeout=5.0)

    navigate_id = await client.send('Page.navigate', {{'url': 'https://www.ifeng.com/'}}, session_id=session_id)
    navigate_response, _ = await client.recv_until_id(navigate_id, timeout=30.0)

    # Wait for DCL
    deadline = time.time() + 10
    while time.time() < deadline:
        try:
            msg = await asyncio.wait_for(client.recv(), timeout=1.0)
            if msg.get('method') == 'Page.domContentEventFired':
                print('DCL received')
                break
        except asyncio.TimeoutError:
            break

    await client.websocket.close()
    print('Done')

asyncio.run(main())
"""
    result = subprocess.run(
        ["uv", "run", "python", "-c", navigate_script],
        capture_output=True,
        text=True,
        timeout=60,
    )
    print(f"Navigate stdout: {result.stdout}")
    if result.stderr:
        print(f"Navigate stderr: {result.stderr[:500]}")

    # Wait for logs to flush
    time.sleep(2)

    # Terminate
    proc.terminate()
    proc.wait(timeout=10)

    # Read and filter stderr
    print("\n=== V8 Isolate Timing Logs ===")
    with open(stderr_path) as f:
        lines = f.readlines()

    print(f"Total log lines: {len(lines)}")

    timing_lines = []
    for line in lines:
        if any(kw in line for kw in [
            "v8_isolate", "isolate_bootstrap", "context_bootstrap",
            "constructor_specs", "build_constructor", "native_bridge",
            "inspector_backend", "define_global", "setup_inheritance"
        ]):
            timing_lines.append(line.rstrip())

    if timing_lines:
        for line in timing_lines:
            print(line)
    else:
        print("No V8 isolate timing logs found.")
        print("\nShowing first 20 lines:")
        for line in lines[:20]:
            print(line.rstrip())

    # Clean up
    os.unlink(stderr_path)


if __name__ == "__main__":
    main()
