#!/usr/bin/env python3
"""Pin Phase 16 collapsed-table border geometry and pixels to Chromium.

Run with:

    uv run --project moli-benchmark --with pillow \
      python scripts/layout-phase16-collapsed-table-border-chromium-differential.py

`--record` prints Chromium's current values while updating the paired Rust
fixture. Normal runs assert the checked-in Chrome 147 oracle.
"""

from __future__ import annotations

import argparse
import asyncio
import base64
import io
import json
import os
import socket
import subprocess
import tempfile
import time
import urllib.parse
from pathlib import Path
from typing import Any

from PIL import Image

from moli_benchmark.raw_cdp import RawCdpClient, connect_raw_cdp


DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()

FIXTURE = r"""<!doctype html><meta charset=utf-8><style>
html,body{margin:0;padding:0;background:white}table{position:absolute;border-collapse:collapse;table-layout:fixed;padding:0}td{box-sizing:border-box;padding:0}
#precedence{left:20px;top:20px;width:100px;border:4px solid rgb(90,0,90)}
#precedence colgroup{border-right:4px solid rgb(0,130,0)}
#precedence col{border-bottom:4px solid rgb(0,0,180)}
#precedence tbody{border-left:4px solid rgb(230,120,0)}
#precedence tr{border-top:4px solid rgb(0,160,160)}
#precedence td{width:50px;height:30px;background:rgb(240,240,240)}

#rules{left:20px;top:90px;width:120px;border:2px solid black}
#rules td{width:60px;height:30px;background:rgb(245,245,210)}
#wide-left{border-right:4px solid rgb(0,0,255)}#wide-right{border-left:8px solid rgb(255,0,0)}
#style-left{border-right:6px dashed rgb(0,150,0)}#style-right{border-left:6px solid rgb(0,0,255)}
#hidden-left{border-right:20px double rgb(120,0,120)}#hidden-right{border-left:1px hidden red}

#span{left:20px;top:220px;width:100px;border:0}
#span td{width:50px;height:30px;padding:0}
#spanning{border:6px solid rgb(0,140,0);background:rgb(210,255,210)}
#upper,#lower{border:2px solid rgb(220,0,0);background:rgb(255,220,220)}

#colspan{left:20px;top:300px;width:100px;border:0}
#colspan td{width:50px;height:30px;padding:0}
#across{border:6px solid rgb(0,140,0);background:rgb(210,255,210)}
#col-left,#col-right{border:2px solid rgb(220,0,0);background:rgb(255,220,220)}
</style>
<table id=precedence><colgroup><col><col></colgroup><tbody><tr><td id=p0></td><td id=p1></td></tr></tbody></table>
<table id=rules><tbody>
<tr><td id=wide-left></td><td id=wide-right></td></tr>
<tr><td id=style-left></td><td id=style-right></td></tr>
<tr><td id=hidden-left></td><td id=hidden-right></td></tr>
</tbody></table>
<table id=span><tbody><tr><td id=spanning rowspan=2></td><td id=upper></td></tr><tr><td id=lower></td></tr></tbody></table>
<table id=colspan><tbody><tr><td id=across colspan=2></td></tr><tr><td id=col-left></td><td id=col-right></td></tr></tbody></table>"""

# Filled from Chromium 147 after the fixture was fixed. Values are CSS pixels.
EXPECTED_GEOMETRY: dict[str, Any] = {
    "precedence": [20, 20, 104, 34],
    "p0": [22, 22, 50, 30],
    "p1": [72, 22, 50, 30],
    "rules": [20, 90, 122, 92],
    "wide-left": [21, 91, 60, 30],
    "wide-right": [81, 91, 60, 30],
    "style-left": [21, 121, 60, 30],
    "style-right": [81, 121, 60, 30],
    "hidden-left": [21, 151, 60, 30],
    "hidden-right": [81, 151, 60, 30],
    "span": [20, 220, 104, 66],
    "spanning": [23, 223, 50, 60],
    "upper": [73, 223, 50, 30],
    "lower": [73, 253, 50, 30],
    "colspan": [20, 300, 100, 64],
    "across": [23, 303, 94, 30],
    "col-left": [23, 333, 47, 30],
    "col-right": [70, 333, 47, 30],
}
EXPECTED_PIXELS: dict[str, list[int]] = {
    "precedence-top": [0, 160, 160, 255],
    "precedence-left": [230, 120, 0, 255],
    "precedence-bottom": [0, 0, 180, 255],
    "precedence-right": [0, 130, 0, 255],
    "width-wins": [255, 0, 0, 255],
    "style-wins": [0, 0, 255, 255],
    "hidden-suppresses": [245, 245, 210, 255],
    "span-interior-clear": [210, 255, 210, 255],
    "span-interior-right": [220, 0, 0, 255],
    "colspan-interior-clear": [210, 255, 210, 255],
    "colspan-interior-lower": [220, 0, 0, 255],
}
SAMPLE_POINTS: dict[str, tuple[int, int]] = {
    "precedence-top": (70, 21),
    "precedence-left": (21, 35),
    "precedence-bottom": (70, 51),
    "precedence-right": (122, 35),
    "width-wins": (80, 105),
    "style-wins": (80, 135),
    "hidden-suppresses": (80, 165),
    "span-interior-clear": (45, 253),
    "span-interior-right": (95, 253),
    "colspan-interior-clear": (70, 318),
    "colspan-interior-lower": (70, 348),
}


def reserve_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


async def command(
    client: RawCdpClient,
    method: str,
    params: dict[str, Any] | None = None,
    *,
    session_id: str | None = None,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    message_id = await client.send(method, params, session_id=session_id)
    response, messages = await client.recv_until_id(message_id, timeout=10.0)
    return dict(response.get("result") or {}), messages


async def wait_for_load(
    client: RawCdpClient, session_id: str, messages: list[dict[str, Any]]
) -> None:
    def loaded(message: dict[str, Any]) -> bool:
        return (
            message.get("sessionId") == session_id
            and message.get("method") == "Page.loadEventFired"
        )

    if any(loaded(message) for message in messages):
        return
    deadline = asyncio.get_running_loop().time() + 10.0
    while asyncio.get_running_loop().time() < deadline:
        message = await asyncio.wait_for(
            client.recv(), timeout=deadline - asyncio.get_running_loop().time()
        )
        if loaded(message):
            return
    raise TimeoutError("Chromium fixture load timed out")


async def evaluate(
    client: RawCdpClient, session_id: str, expression: str
) -> dict[str, Any]:
    result, _ = await command(
        client,
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True, "awaitPromise": True},
        session_id=session_id,
    )
    if result.get("exceptionDetails") is not None:
        raise RuntimeError(f"Runtime.evaluate failed: {result['exceptionDetails']}")
    return dict(result["result"]["value"])


async def screenshot(client: RawCdpClient, session_id: str) -> Image.Image:
    result, _ = await command(
        client,
        "Page.captureScreenshot",
        {"format": "png", "fromSurface": True},
        session_id=session_id,
    )
    return Image.open(io.BytesIO(base64.b64decode(result["data"]))).convert("RGBA")


def assert_value(label: str, actual: Any, expected: Any) -> None:
    if isinstance(expected, (int, float)) and not isinstance(expected, bool):
        if not isinstance(actual, (int, float)) or abs(actual - expected) > 0.05:
            raise AssertionError(f"{label}: expected {expected}, got {actual}")
        return
    if isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            raise AssertionError(f"{label}: expected {expected}, got {actual}")
        for index, (actual_item, expected_item) in enumerate(zip(actual, expected)):
            assert_value(f"{label}[{index}]", actual_item, expected_item)
        return
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or actual.keys() != expected.keys():
            raise AssertionError(f"{label}: expected keys {expected.keys()}, got {actual}")
        for key, expected_item in expected.items():
            assert_value(f"{label}.{key}", actual[key], expected_item)
        return
    if actual != expected:
        raise AssertionError(f"{label}: expected {expected!r}, got {actual!r}")


async def measure(client: RawCdpClient, session_id: str, *, record: bool) -> dict[str, Any]:
    await command(
        client,
        "Emulation.setDeviceMetricsOverride",
        {"width": 180, "height": 380, "deviceScaleFactor": 1, "mobile": False},
        session_id=session_id,
    )
    _, messages = await command(
        client,
        "Page.navigate",
        {"url": "data:text/html;charset=utf-8," + urllib.parse.quote(FIXTURE)},
        session_id=session_id,
    )
    await wait_for_load(client, session_id, messages)
    geometry = await evaluate(
        client,
        session_id,
        """(async()=>{await new Promise(requestAnimationFrame);await new Promise(requestAnimationFrame);
const r=id=>{const x=document.getElementById(id).getBoundingClientRect();return [x.x,x.y,x.width,x.height]};
return Object.fromEntries(['precedence','p0','p1','rules','wide-left','wide-right','style-left','style-right','hidden-left','hidden-right','span','spanning','upper','lower','colspan','across','col-left','col-right'].map(id=>[id,r(id)]));})()""",
    )
    image = await screenshot(client, session_id)
    points = SAMPLE_POINTS
    pixels = {name: list(image.getpixel(point)) for name, point in points.items()}
    if not record:
        assert_value("geometry", geometry, EXPECTED_GEOMETRY)
        assert_value("pixels", pixels, EXPECTED_PIXELS)
    return {"geometry": geometry, "samplePoints": points, "pixels": pixels}


async def run(binary: Path, *, record: bool) -> dict[str, Any]:
    port = reserve_port()
    endpoint = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(
        prefix="moli-layout-phase16-collapsed-border-chromium-",
        ignore_cleanup_errors=True,
    ) as profile:
        process = subprocess.Popen(
            [
                str(binary),
                "--headless=new",
                "--disable-background-networking",
                "--disable-default-apps",
                "--disable-gpu",
                "--hide-scrollbars",
                "--no-first-run",
                "--no-sandbox",
                "--remote-debugging-address=127.0.0.1",
                f"--remote-debugging-port={port}",
                f"--user-data-dir={profile}",
                "about:blank",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        client: RawCdpClient | None = None
        target_id: str | None = None
        try:
            deadline = time.monotonic() + 10.0
            last_error: BaseException | None = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"Chromium exited during startup: {process.returncode}")
                try:
                    client = await connect_raw_cdp(endpoint)
                    break
                except BaseException as error:
                    last_error = error
                    await asyncio.sleep(0.05)
            if client is None:
                raise TimeoutError(f"Chromium CDP startup timed out: {last_error!r}")
            version, _ = await command(client, "Browser.getVersion")
            target, _ = await command(client, "Target.createTarget", {"url": "about:blank"})
            target_id = str(target["targetId"])
            attached, _ = await command(
                client, "Target.attachToTarget", {"targetId": target_id, "flatten": True}
            )
            session_id = str(attached["sessionId"])
            await command(client, "Page.enable", session_id=session_id)
            measured = await measure(client, session_id, record=record)
            return {
                "status": "recorded" if record else "passed",
                "product": version.get("product"),
                "revision": version.get("revision"),
                **measured,
            }
        finally:
            if client is not None:
                if target_id is not None:
                    try:
                        await command(client, "Target.closeTarget", {"targetId": target_id})
                    except BaseException:
                        pass
                await client.websocket.close()
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=3.0)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=3.0)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", type=Path, default=DEFAULT_CHROMIUM)
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    binary = args.chromium.expanduser().resolve()
    if not binary.is_file():
        raise SystemExit(f"Chromium binary does not exist: {binary}")
    print(json.dumps(asyncio.run(run(binary, record=args.record)), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
