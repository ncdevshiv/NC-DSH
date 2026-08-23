#!/usr/bin/env python3
"""Pin Phase 19 CSS gradient domains and interpolation to Chromium.

Run with:

    uv run --project moli-benchmark --with pillow \
      python scripts/layout-phase19-gradient-chromium-differential.py

The paired Rust regression is
`screenshot_resolves_css_gradient_domains_hints_and_interpolation_like_chromium`.
Use `--record` when intentionally refreshing the Chromium pixel oracle.
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
html,body{margin:0;padding:0;background:white}.case{position:absolute;width:100px;height:100px}
#negative{left:0;top:0;background:linear-gradient(to bottom,rgba(0,0,0,.5) -20%,transparent 30%),white}
#overflow{left:110px;top:0;background:linear-gradient(to bottom,rgb(255,0,0) 80%,rgb(0,0,255) 120%)}
#repeat{left:220px;top:0;background:repeating-linear-gradient(to bottom,rgb(255,0,0) -20%,rgb(0,0,255) 30%)}
#radial-overflow{left:330px;top:0;background:radial-gradient(circle 50px at 50px 50px,rgb(255,0,0) 80%,rgb(0,0,255) 120%)}
#radial{left:0;top:110px;background:radial-gradient(circle 50px at 50px 50px,rgb(255,0,0) -20%,rgb(0,0,255) 30%)}
#conic{left:110px;top:110px;background:conic-gradient(from 0deg at 50px 50px,rgb(255,0,0) -20%,rgb(0,0,255) 30%)}
#hint{left:220px;top:110px;background:linear-gradient(to right,rgb(255,0,0) 0%,25%,rgb(0,0,255) 100%)}
#conic-overflow{left:330px;top:110px;background:conic-gradient(from 0deg at 50px 50px,rgb(255,0,0) 80%,rgb(0,0,255) 120%)}
#degenerate{left:0;top:220px;background:linear-gradient(to right,rgb(255,0,0) -20%,rgb(0,0,255) -20%)}
#repeat-degenerate{left:110px;top:220px;background:repeating-linear-gradient(to right,rgb(255,0,0) 20%,rgb(0,0,255) 20%)}
#oklab{left:220px;top:220px;background:linear-gradient(to right in oklab,rgb(255,0,0),rgb(0,0,255))}
#repeat-radial{left:330px;top:220px;background:repeating-radial-gradient(circle 50px at 50px 50px,rgb(255,0,0) -20%,rgb(0,0,255) 30%)}
#repeat-conic{left:0;top:330px;background:repeating-conic-gradient(from 0deg at 50px 50px,rgb(255,0,0) -20%,rgb(0,0,255) 30%)}
#p3-linear{left:110px;top:330px;background:linear-gradient(to right in display-p3-linear,rgb(255,0,0),rgb(0,0,255))}
#normal-conic{left:220px;top:330px;background:conic-gradient(from 0deg at 50px 50px,rgb(255,0,0) 0%,rgb(0,0,255) 100%)}
</style>
<div id=negative class=case></div><div id=overflow class=case></div><div id=repeat class=case></div><div id=radial-overflow class=case></div>
<div id=radial class=case></div><div id=conic class=case></div><div id=hint class=case></div><div id=conic-overflow class=case></div>
<div id=degenerate class=case></div><div id=repeat-degenerate class=case></div><div id=oklab class=case></div><div id=repeat-radial class=case></div>
<div id=repeat-conic class=case></div><div id=p3-linear class=case></div><div id=normal-conic class=case></div>"""

SAMPLE_POINTS: dict[str, tuple[int, int]] = {
    "negative-start": (50, 0),
    "negative-middle": (50, 10),
    "negative-end": (50, 29),
    "negative-padded": (50, 50),
    "overflow-padded": (160, 50),
    "overflow-start": (160, 80),
    "overflow-middle": (160, 99),
    "repeat-first": (270, 0),
    "repeat-transition": (270, 10),
    "repeat-end": (270, 29),
    "repeat-next-period": (270, 30),
    "radial-center": (50, 160),
    "radial-transition": (60, 160),
    "radial-padded": (70, 160),
    "conic-top": (160, 120),
    "conic-right": (200, 160),
    "conic-bottom": (160, 200),
    "conic-left": (120, 160),
    "hint-quarter": (245, 160),
    "hint-middle": (270, 160),
    "hint-third-quarter": (295, 160),
    "degenerate": (50, 270),
    "repeat-degenerate": (160, 270),
    "oklab-quarter": (245, 270),
    "oklab-middle": (270, 270),
    "oklab-third-quarter": (295, 270),
    "radial-overflow-pad": (419, 50),
    "radial-overflow-start": (420, 50),
    "radial-overflow-middle": (429, 50),
    "conic-overflow-pad": (380, 120),
    "conic-overflow-middle": (350, 130),
    "repeat-radial-center": (380, 270),
    "repeat-radial-next-period": (400, 270),
    "repeat-conic-top": (50, 340),
    "repeat-conic-right": (90, 380),
    "repeat-conic-bottom": (50, 420),
    "display-p3-linear-quarter": (135, 380),
    "display-p3-linear-middle": (160, 380),
    "display-p3-linear-third-quarter": (185, 380),
    "normal-conic-top": (270, 340),
    "normal-conic-right": (310, 380),
    "normal-conic-bottom": (270, 420),
    "normal-conic-left": (230, 380),
}

EXPECTED_PIXELS: dict[str, list[int]] = {
    "negative-start": [179, 179, 179, 255],
    "negative-middle": [205, 205, 205, 255],
    "negative-end": [254, 254, 254, 255],
    "negative-padded": [255, 255, 255, 255],
    "overflow-padded": [255, 0, 0, 255],
    "overflow-start": [251, 0, 3, 255],
    "overflow-middle": [131, 0, 124, 255],
    "repeat-first": [150, 0, 104, 255],
    "repeat-transition": [99, 0, 155, 255],
    "repeat-end": [3, 0, 253, 255],
    "repeat-next-period": [252, 0, 2, 255],
    "radial-center": [145, 0, 109, 255],
    "radial-transition": [45, 0, 209, 255],
    "radial-padded": [0, 0, 255, 255],
    "conic-top": [151, 0, 103, 255],
    "conic-right": [24, 0, 230, 255],
    "conic-bottom": [0, 0, 255, 255],
    "conic-left": [0, 0, 255, 255],
    "hint-quarter": [127, 0, 129, 255],
    "hint-middle": [74, 0, 181, 255],
    "hint-third-quarter": [36, 0, 220, 255],
    "degenerate": [0, 0, 255, 255],
    "repeat-degenerate": [0, 0, 255, 255],
    "oklab-quarter": [197, 74, 111, 255],
    "oklab-middle": [139, 83, 163, 255],
    "oklab-third-quarter": [80, 71, 211, 255],
    "radial-overflow-pad": [255, 0, 0, 255],
    "radial-overflow-start": [248, 0, 6, 255],
    "radial-overflow-middle": [134, 0, 121, 255],
    "conic-overflow-pad": [255, 0, 0, 255],
    "conic-overflow-middle": [207, 0, 48, 255],
    "repeat-radial-center": [145, 0, 109, 255],
    "repeat-radial-next-period": [198, 0, 56, 255],
    "repeat-conic-top": [152, 0, 103, 255],
    "repeat-conic-right": [24, 0, 230, 255],
    "repeat-conic-bottom": [154, 0, 101, 255],
    "display-p3-linear-quarter": [224, 0, 138, 255],
    "display-p3-linear-middle": [186, 0, 188, 255],
    "display-p3-linear-third-quarter": [136, 0, 226, 255],
    "normal-conic-top": [254, 0, 0, 255],
    "normal-conic-right": [190, 0, 64, 255],
    "normal-conic-bottom": [128, 0, 127, 255],
    "normal-conic-left": [64, 0, 190, 255],
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


async def screenshot(client: RawCdpClient, session_id: str) -> Image.Image:
    result, _ = await command(
        client,
        "Page.captureScreenshot",
        {"format": "png", "fromSurface": True},
        session_id=session_id,
    )
    return Image.open(io.BytesIO(base64.b64decode(result["data"]))).convert("RGBA")


async def measure(
    client: RawCdpClient, session_id: str, *, record: bool
) -> dict[str, Any]:
    await command(
        client,
        "Emulation.setDeviceMetricsOverride",
        {"width": 430, "height": 430, "deviceScaleFactor": 1, "mobile": False},
        session_id=session_id,
    )
    _, messages = await command(
        client,
        "Page.navigate",
        {"url": "data:text/html;charset=utf-8," + urllib.parse.quote(FIXTURE)},
        session_id=session_id,
    )
    await wait_for_load(client, session_id, messages)
    image = await screenshot(client, session_id)
    pixels = {
        name: list(image.getpixel(point)) for name, point in SAMPLE_POINTS.items()
    }
    if not record:
        mismatches = {
            name: {"expected": EXPECTED_PIXELS[name], "actual": actual}
            for name, actual in pixels.items()
            if any(
                abs(actual_channel - expected_channel) > 1
                for actual_channel, expected_channel in zip(
                    actual, EXPECTED_PIXELS[name]
                )
            )
        }
        if mismatches:
            raise AssertionError(f"Chromium gradient oracle changed: {mismatches}")
    return {"samplePoints": SAMPLE_POINTS, "pixels": pixels}


async def run(binary: Path, *, record: bool) -> dict[str, Any]:
    port = reserve_port()
    endpoint = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(
        prefix="moli-layout-phase19-gradient-chromium-",
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
                raise TimeoutError(f"Chromium CDP startup timed out: {last_error!r}")
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
    print(
        json.dumps(
            asyncio.run(run(binary, record=args.record)),
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
