#!/usr/bin/env python3
"""Validate Phase 15 iframe viewport and pixel-composition semantics in Chromium.

Run with:

    uv run --project moli-benchmark --with pillow \
      python scripts/layout-phase15-iframe-chromium-differential.py

The paired Rust regression is
`screenshot_composes_iframe_documents_into_exact_used_content_viewports`.
Both fixtures exercise the used content box, viewport units, clipping, nested
documents, transforms, and child-document scrolling.
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
html{margin:0;padding:0;background:white}body{margin:0;padding:0}
</style><script>
addEventListener('DOMContentLoaded', () => {
  const frame = document.createElement('iframe');
  frame.id = 'paint-frame';
  frame.style.cssText = 'position:absolute;left:20px;top:10px;display:block;box-sizing:border-box;width:120px;height:80px;margin:0;border:4px solid black;padding:6px;background:rgb(255,255,0);transform:translate(5px,3px)';
  document.body.appendChild(frame);

  const child = frame.contentDocument;
  child.documentElement.style.cssText = 'margin:0;padding:0;background:rgb(0,255,255)';
  child.body.style.cssText = 'position:relative;margin:0;padding:0;width:200px;height:120px';
  const viewportSized = child.createElement('div');
  viewportSized.id = 'viewport-sized';
  viewportSized.style.cssText = 'position:absolute;left:0;top:0;width:50vw;height:50vh;background:rgb(255,0,0)';
  child.body.appendChild(viewportSized);
  const clipped = child.createElement('div');
  clipped.style.cssText = 'position:absolute;left:90px;top:0;width:30px;height:100px;background:rgb(0,128,0)';
  child.body.appendChild(clipped);
  const label = child.createElement('span');
  label.textContent = 'frame';
  label.style.cssText = 'position:absolute;left:55px;top:35px;font:10px/10px sans-serif;color:black';
  child.body.appendChild(label);
  const icon = child.createElementNS('http://www.w3.org/2000/svg', 'svg');
  icon.setAttribute('width', '8');
  icon.setAttribute('height', '8');
  icon.style.cssText = 'position:absolute;left:75px;top:45px';
  const iconRect = child.createElementNS('http://www.w3.org/2000/svg', 'rect');
  iconRect.setAttribute('width', '8');
  iconRect.setAttribute('height', '8');
  iconRect.setAttribute('fill', 'rgb(128,0,128)');
  icon.appendChild(iconRect);
  child.body.appendChild(icon);

  const nested = child.createElement('iframe');
  nested.id = 'nested-frame';
  nested.style.cssText = 'position:absolute;left:10px;top:35px;display:block;box-sizing:border-box;width:40px;height:20px;margin:0;border:2px solid rgb(0,0,255);padding:2px;background:rgb(255,255,0)';
  child.body.appendChild(nested);
  const nestedDocument = nested.contentDocument;
  nestedDocument.documentElement.style.cssText = 'margin:0;padding:0;background:rgb(255,0,255)';
  nestedDocument.body.style.cssText = 'position:relative;margin:0;padding:0;width:80px;height:40px';
  const nestedViewportSized = nestedDocument.createElement('div');
  nestedViewportSized.id = 'nested-viewport-sized';
  nestedViewportSized.style.cssText = 'width:50vw;height:100vh;background:rgb(0,0,0)';
  nestedDocument.body.appendChild(nestedViewportSized);
});
</script>"""

EXPECTED_GEOMETRY: dict[str, Any] = {
    "frameRect": [25, 13, 120, 80],
    "childViewport": [100, 60],
    "viewportSizedRect": [0, 0, 50, 30],
    "nestedRect": [10, 35, 40, 20],
    "nestedViewport": [32, 12],
    "nestedViewportSizedRect": [0, 0, 16, 12],
}

EXPECTED_INITIAL_PIXELS = {
    "parent-canvas": [255, 255, 255, 255],
    "frame-border": [0, 0, 0, 255],
    "frame-padding": [255, 255, 0, 255],
    "child-vw": [255, 0, 0, 255],
    "child-canvas": [0, 255, 255, 255],
    "child-clipped": [0, 128, 0, 255],
    "nested-border": [0, 0, 255, 255],
    "nested-padding": [255, 255, 0, 255],
    "nested-vw": [0, 0, 0, 255],
    "nested-canvas": [255, 0, 255, 255],
}

INITIAL_SAMPLE_POINTS = {
    "parent-canvas": (146, 30),
    "frame-border": (26, 14),
    "frame-padding": (31, 20),
    "child-vw": (84, 24),
    "child-canvas": (85, 24),
    "child-clipped": (130, 30),
    "nested-border": (46, 59),
    "nested-padding": (48, 61),
    "nested-vw": (64, 63),
    "nested-canvas": (65, 63),
}

EXPECTED_SCROLLED_PIXELS = {
    "child-vw": [255, 0, 0, 255],
    "child-canvas": [0, 255, 255, 255],
    "child-shifted-overflow": [0, 128, 0, 255],
    "frame-padding": [255, 255, 0, 255],
}

SCROLLED_SAMPLE_POINTS = {
    "child-vw": (60, 24),
    "child-canvas": (70, 24),
    "child-shifted-overflow": (110, 24),
    "frame-padding": (136, 24),
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
    client: RawCdpClient,
    session_id: str,
    messages: list[dict[str, Any]],
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


def rect(value: dict[str, Any]) -> list[float]:
    return [value[key] for key in ("x", "y", "width", "height")]


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


def sample(image: Image.Image, points: dict[str, tuple[int, int]]) -> dict[str, list[int]]:
    return {name: list(image.getpixel(point)) for name, point in points.items()}


async def measure(client: RawCdpClient, session_id: str, *, record: bool) -> dict[str, Any]:
    await command(
        client,
        "Emulation.setDeviceMetricsOverride",
        {"width": 180, "height": 120, "deviceScaleFactor": 1, "mobile": False},
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
const frame=document.getElementById('paint-frame');const child=frame.contentDocument;
const nested=child.getElementById('nested-frame');const nestedDocument=nested.contentDocument;
const r=value=>{const x=value.getBoundingClientRect();return [x.x,x.y,x.width,x.height]};
return {frameRect:r(frame),childViewport:[frame.contentWindow.innerWidth,frame.contentWindow.innerHeight],
viewportSizedRect:r(child.getElementById('viewport-sized')),nestedRect:r(nested),
nestedViewport:[nested.contentWindow.innerWidth,nested.contentWindow.innerHeight],
nestedViewportSizedRect:r(nestedDocument.getElementById('nested-viewport-sized'))};})()""",
    )
    initial_pixels = sample(await screenshot(client, session_id), INITIAL_SAMPLE_POINTS)
    await evaluate(
        client,
        session_id,
        """(async()=>{document.getElementById('paint-frame').contentWindow.scrollTo(20,10);
await new Promise(requestAnimationFrame);return {scrollX:document.getElementById('paint-frame').contentWindow.scrollX,
scrollY:document.getElementById('paint-frame').contentWindow.scrollY};})()""",
    )
    scrolled_pixels = sample(await screenshot(client, session_id), SCROLLED_SAMPLE_POINTS)
    if not record:
        assert_value("geometry", geometry, EXPECTED_GEOMETRY)
        assert_value("initialPixels", initial_pixels, EXPECTED_INITIAL_PIXELS)
        assert_value("scrolledPixels", scrolled_pixels, EXPECTED_SCROLLED_PIXELS)
    return {
        "geometry": geometry,
        "initialPixels": initial_pixels,
        "scrolledPixels": scrolled_pixels,
    }


async def run(binary: Path, *, record: bool) -> dict[str, Any]:
    port = reserve_port()
    endpoint = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(
        prefix="moli-layout-phase15-iframe-chromium-", ignore_cleanup_errors=True
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
