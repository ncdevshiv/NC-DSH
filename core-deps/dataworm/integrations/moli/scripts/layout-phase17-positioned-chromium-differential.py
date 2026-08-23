#!/usr/bin/env python3
"""Pin Phase 17 positioned-layout geometry and pixels to Chromium.

Run with:

    uv run --project moli-benchmark --with pillow \
      python scripts/layout-phase17-positioned-chromium-differential.py

The fixture covers CSS Positioned Layout auto-margin distribution and relative
percentage/calc inset resolution for definite and indefinite block sizes. The
paired Rust regression is
`positioned_layout_matches_chromium_auto_margin_and_relative_inset_rules`.
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
html,body{margin:0;padding:0;font-size:16px;background:white}
#centered{position:fixed;left:0;right:0;top:0;width:975px;height:20px;margin-left:auto;margin-right:auto;background:red}
#definite-parent{position:absolute;left:0;top:30px;width:100px;height:400px}
#definite-child{position:relative;top:calc(max(120px,100% - 12.6875rem));width:10px;height:10px;background:blue}
#indefinite-parent{position:absolute;left:0;top:500px;width:100px;min-height:100px}
#indefinite-child{position:relative;top:calc(10px + 10%);width:10px;height:100px;background:lime}
</style>
<div id=centered></div>
<div id=definite-parent><div id=definite-child></div></div>
<div id=indefinite-parent><div id=indefinite-child></div></div>"""

EXPECTED_GEOMETRY: dict[str, list[float]] = {
    "centered": [232.5, 0, 975, 20],
    "definite-parent": [0, 30, 100, 400],
    "definite-child": [0, 227, 10, 10],
    "indefinite-parent": [0, 500, 100, 100],
    "indefinite-child": [0, 500, 10, 100],
}
SAMPLE_POINTS: dict[str, tuple[int, int]] = {
    "before-centered": (231, 10),
    "inside-centered": (234, 10),
    "definite-relative": (5, 230),
    "indefinite-relative": (5, 505),
}
EXPECTED_PIXELS: dict[str, list[int]] = {
    "before-centered": [255, 255, 255, 255],
    "inside-centered": [255, 0, 0, 255],
    "definite-relative": [0, 0, 255, 255],
    "indefinite-relative": [0, 255, 0, 255],
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


async def capture(client: RawCdpClient, session_id: str) -> Image.Image:
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


async def measure(
    client: RawCdpClient, session_id: str, *, record: bool
) -> dict[str, Any]:
    await command(
        client,
        "Emulation.setDeviceMetricsOverride",
        {"width": 1440, "height": 620, "deviceScaleFactor": 1, "mobile": False},
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
const rect=id=>{const r=document.getElementById(id).getBoundingClientRect();return [r.x,r.y,r.width,r.height]};
return Object.fromEntries(['centered','definite-parent','definite-child','indefinite-parent','indefinite-child'].map(id=>[id,rect(id)]));})()""",
    )
    image = await capture(client, session_id)
    pixels = {
        name: list(image.getpixel(point)) for name, point in SAMPLE_POINTS.items()
    }
    if not record:
        assert_value("geometry", geometry, EXPECTED_GEOMETRY)
        assert_value("pixels", pixels, EXPECTED_PIXELS)
    return {"geometry": geometry, "samplePoints": SAMPLE_POINTS, "pixels": pixels}


async def run(binary: Path, *, record: bool) -> dict[str, Any]:
    port = reserve_port()
    endpoint = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(
        prefix="moli-layout-phase17-positioned-chromium-",
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
                    raise RuntimeError(
                        f"Chromium exited during startup: {process.returncode}"
                    )
                try:
                    client = await connect_raw_cdp(endpoint)
                    break
                except BaseException as error:
                    last_error = error
                    await asyncio.sleep(0.05)
            if client is None:
                raise TimeoutError(
                    f"Chromium CDP startup timed out: {last_error!r}"
                )
            version, _ = await command(client, "Browser.getVersion")
            target, _ = await command(
                client, "Target.createTarget", {"url": "about:blank"}
            )
            target_id = str(target["targetId"])
            attached, _ = await command(
                client,
                "Target.attachToTarget",
                {"targetId": target_id, "flatten": True},
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
                        await command(
                            client, "Target.closeTarget", {"targetId": target_id}
                        )
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
