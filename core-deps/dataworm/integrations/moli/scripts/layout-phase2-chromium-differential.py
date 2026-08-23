#!/usr/bin/env python3
"""Validate the Phase 2 CSS-pixel corpus against local Chromium.

Run with:

    uv run --project moli-benchmark \
      python scripts/layout-phase2-chromium-differential.py

The Rust layout contract and renderer tests assert the same coordinates. This
script deliberately measures DOM geometry rather than comparing screenshots so
font and raster differences cannot hide a main-layout regression.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import socket
import subprocess
import tempfile
import time
import urllib.parse
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from moli_benchmark.raw_cdp import RawCdpClient, connect_raw_cdp


DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()
TOLERANCE = 0.01


@dataclass(frozen=True)
class Case:
    name: str
    width: int
    height: int
    html: str
    expected_rects: dict[str, list[float]]
    expected_document_size: list[float] | None = None


CASES = (
    Case(
        name="grid-calc-positioned",
        width=800,
        height=600,
        html="""<!doctype html><style>
html,body{display:block;margin:0;padding:0}
#grid{display:grid;width:400px;height:200px;gap:20px 10px;grid-template-columns:100px 1fr;grid-template-rows:50px 1fr;grid-auto-rows:30px}
#first{grid-column:1;grid-row:1}#second{grid-column:2;grid-row:1/3}#implicit{grid-column:1;grid-row:3}
#calc{width:calc(50% - 10px);height:20px}
#positioned{box-sizing:content-box;position:relative;margin:30px;width:400px;height:300px;padding:20px;border:5px solid transparent}
#static{margin:10px;width:200px;height:100px}
#absolute{position:absolute;left:10%;top:25%;width:50%;height:10px}
#fixed{position:fixed;right:10px;bottom:20px;width:30px;height:40px}
</style><div id=grid><div id=first></div><div id=second></div><div id=implicit></div></div><div id=calc></div><div id=positioned><div id=static><div id=absolute></div></div><div id=fixed></div></div>""",
        expected_rects={
            "first": [0.0, 0.0, 100.0, 50.0],
            "second": [110.0, 0.0, 290.0, 150.0],
            "implicit": [0.0, 170.0, 100.0, 30.0],
            "calc": [0.0, 200.0, 390.0, 20.0],
            "positioned": [30.0, 250.0, 450.0, 350.0],
            "absolute": [79.0, 340.0, 220.0, 10.0],
            "fixed": [760.0, 540.0, 30.0, 40.0],
        },
        expected_document_size=[800.0, 630.0],
    ),
    Case(
        name="static-inset-flex-order",
        width=800,
        height=600,
        html="""<!doctype html><style>
html,body{margin:0;padding:0}
#static{position:static;left:80px;top:40px;width:20px;height:10px}
#flex{display:flex;width:300px;height:40px;align-items:center}
#late{order:2;flex:1 1 100px;height:20px}#early{order:-1;flex:3 1 100px;height:40px}
#cb{position:relative;width:200px;height:100px}#mid{margin:10px;width:80px;height:30px}#auto-static{position:absolute;width:20px;height:10px}
#intrinsic{width:min-content;height:10px}#basis{display:flex}#basis-child{flex:0 0 content;min-width:0;width:10px;height:10px}#basis-content{width:80px;height:10px}
#clamped{width:90%;max-width:240px;height:10px;margin-left:30px}
</style><div id=static></div><div id=flex><div id=late></div><div id=early></div></div><div id=cb><div id=mid><div id=auto-static></div></div></div><div id=intrinsic></div><div id=basis><div id=basis-child><div id=basis-content></div></div></div><div id=clamped></div>""",
        expected_rects={
            "static": [0.0, 0.0, 20.0, 10.0],
            "early": [0.0, 10.0, 175.0, 40.0],
            "late": [175.0, 20.0, 125.0, 20.0],
            "basis-child": [0.0, 170.0, 80.0, 10.0],
            "clamped": [30.0, 180.0, 240.0, 10.0],
        },
    ),
    Case(
        name="flex-basis-content-aspect-ratio",
        width=800,
        height=600,
        html="""<!doctype html><style>
html,body{margin:0}.row{display:flex;width:300px}.content-row{height:50px;align-items:flex-start}
#content-item{flex:0 0 content;min-width:0;width:10px;height:10px}#content-inner{width:80px;height:10px}
.ratio-row{height:40px}.ratio{flex:0 0 content;min-width:0;width:20px;aspect-ratio:2}
#plain,#explicit{align-self:flex-start}#explicit{height:30px}#stretched{align-self:stretch}
.edge-row{height:50px}.edge{flex:0 0 content;min-width:0;width:20px;aspect-ratio:2;padding:5px 7px;border:3px solid;box-sizing:content-box}
#content-box-start{align-self:flex-start;height:30px}#content-box-stretch{align-self:stretch}
#border-box-start{align-self:flex-start;height:30px;box-sizing:border-box}
</style><div class="row content-row"><div id=content-item><div id=content-inner></div></div></div>
<div class="row ratio-row"><div id=plain class=ratio></div></div>
<div class="row ratio-row"><div id=explicit class=ratio></div></div>
<div class="row ratio-row"><div id=stretched class=ratio></div></div>
<div class="row edge-row"><div id=content-box-start class=edge></div></div>
<div class="row edge-row"><div id=content-box-stretch class=edge></div></div>
<div class="row edge-row"><div id=border-box-start class=edge></div></div>""",
        expected_rects={
            "content-item": [0.0, 0.0, 80.0, 10.0],
            "plain": [0.0, 50.0, 0.0, 0.0],
            "explicit": [0.0, 90.0, 60.0, 30.0],
            "stretched": [0.0, 130.0, 80.0, 40.0],
            "content-box-start": [0.0, 170.0, 80.0, 46.0],
            "content-box-stretch": [0.0, 220.0, 88.0, 50.0],
            "border-box-start": [0.0, 270.0, 60.0, 30.0],
        },
    ),
    Case(
        name="block-margin-collapse",
        width=800,
        height=600,
        html="""<!doctype html><style>
html,body{margin:0}#root{width:400px}#first{height:20px;margin:30px 0 40px}#second{height:10px;margin:20px 0 50px}#after{height:5px}
</style><div id=root><div id=first></div><div id=second></div></div><div id=after></div>""",
        expected_rects={
            "root": [0.0, 30.0, 400.0, 70.0],
            "first": [0.0, 30.0, 400.0, 20.0],
            "second": [0.0, 90.0, 400.0, 10.0],
            "after": [0.0, 150.0, 800.0, 5.0],
        },
    ),
    Case(
        name="aspect-ratio-absolute-auto-size",
        width=800,
        height=600,
        html="""<!doctype html><style>
html,body{margin:0}#positioned{position:relative;box-sizing:content-box;width:400px;height:300px;padding:20px;border:5px solid}
#ratio{width:120px;aspect-ratio:2}#absolute{position:absolute;left:10px;right:30px;top:20px;bottom:40px}
</style><div id=positioned><div id=ratio></div><div id=absolute></div></div>""",
        expected_rects={
            "positioned": [0.0, 0.0, 450.0, 350.0],
            "ratio": [25.0, 25.0, 120.0, 60.0],
            "absolute": [15.0, 25.0, 400.0, 280.0],
        },
    ),
    Case(
        name="document-extent-excludes-fixed",
        width=300,
        height=200,
        html="""<!doctype html><style>
html,body{margin:0}#root{position:relative}#flow{width:20px;height:500px}#absolute{position:absolute;left:0;top:700px;width:20px;height:50px}#fixed{position:fixed;left:0;top:900px;width:20px;height:100px}#fixed-child{width:600px;height:300px}
</style><div id=root><div id=flow></div><div id=absolute></div><div id=fixed><div id=fixed-child></div></div></div>""",
        expected_rects={
            "flow": [0.0, 0.0, 20.0, 500.0],
            "absolute": [0.0, 700.0, 20.0, 50.0],
            "fixed": [0.0, 900.0, 20.0, 100.0],
        },
        expected_document_size=[300.0, 750.0],
    ),
    Case(
        name="taffy-013-inline-alignment",
        width=800,
        height=600,
        html="""<!doctype html><style>
html,body{margin:0;padding:0}
.host{width:200px;height:40px;font-size:0;line-height:0}
.atom{display:inline-block;vertical-align:top;width:20px;height:10px;position:relative}
#ltr-atomic{left:10%;right:40px;top:25%;bottom:20px}
#rtl{direction:rtl;text-align:left}#rtl-atomic{direction:ltr;left:10px;right:10%;top:auto;bottom:5px}
#right-row{display:flex;width:200px;height:20px;justify-content:right}#right-item{width:20px;height:10px}
#column-row{display:flex;flex-direction:column;width:50px;height:100px;justify-content:right}#column-item{width:20px;height:20px}
#self-grid{display:grid;width:200px;height:20px}#self-item{width:20px;height:10px;direction:rtl;justify-self:self-start}
</style><div id=ltr class=host><div id=ltr-atomic class=atom></div></div>
<div id=rtl class=host><div id=rtl-atomic class=atom></div></div>
<div id=right-row><div id=right-item></div></div>
<div id=column-row><div id=column-item></div></div>
<div id=self-grid><div id=self-item></div></div>""",
        expected_rects={
            "ltr-atomic": [20.0, 10.0, 20.0, 10.0],
            "rtl-atomic": [-20.0, 35.0, 20.0, 10.0],
            "right-item": [180.0, 80.0, 20.0, 10.0],
            "column-item": [0.0, 100.0, 20.0, 20.0],
            "self-item": [180.0, 200.0, 20.0, 10.0],
        },
    ),
    Case(
        name="taffy-013-flow-root-grid-areas",
        width=800,
        height=600,
        html="""<!doctype html><style>
html,body{margin:0;padding:0}
#flow{display:flow-root;width:100px}#float{float:left;width:40px;height:30px}#after{width:10px;height:5px}
#areas{display:grid;width:300px;height:40px;grid-template-areas:'a a b';grid-template-columns:50px 100px 150px;grid-template-rows:40px}
#area-a{grid-area:a}#area-b{grid-area:b}
</style><div id=flow><div id=float></div></div><div id=after></div>
<div id=areas><div id=area-a></div><div id=area-b></div></div>""",
        expected_rects={
            "flow": [0.0, 0.0, 100.0, 30.0],
            "float": [0.0, 0.0, 40.0, 30.0],
            "after": [0.0, 30.0, 10.0, 5.0],
            "area-a": [0.0, 35.0, 150.0, 40.0],
            "area-b": [150.0, 35.0, 150.0, 40.0],
        },
    ),
    Case(
        name="taffy-013-table-calc-degenerate-ratio",
        width=800,
        height=600,
        html="""<!doctype html><style>
html,body{margin:0;padding:0}table{border-spacing:0;width:300px;table-layout:fixed}td{padding:0;border:0;height:10px}
#calc-cell{width:calc(50% - 20px)}#image{display:block;width:120px;height:auto;aspect-ratio:0/1}
</style><table><tr><td id=calc-cell></td><td id=remaining-cell></td></tr></table>
<svg id=image width=80 height=40 viewBox='0 0 80 40'></svg>""",
        expected_rects={
            "calc-cell": [0.0, 0.0, 150.0, 10.0],
            "remaining-cell": [150.0, 0.0, 150.0, 10.0],
            "image": [0.0, 10.0, 120.0, 60.0],
        },
    ),
)


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
    while True:
        remaining = deadline - asyncio.get_running_loop().time()
        if remaining <= 0:
            raise TimeoutError("timed out waiting for Page.loadEventFired")
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        if loaded(message):
            return


def assert_close(case: str, key: str, actual: list[float], expected: list[float]) -> None:
    if len(actual) != len(expected):
        raise AssertionError(f"{case}.{key}: expected {expected}, got {actual}")
    for index, (actual_value, expected_value) in enumerate(zip(actual, expected)):
        if abs(actual_value - expected_value) > TOLERANCE:
            raise AssertionError(
                f"{case}.{key}[{index}]: expected {expected_value}, got {actual_value}"
            )


async def measure_case(
    client: RawCdpClient,
    session_id: str,
    case: Case,
    *,
    device_scale_factor: float = 1.0,
) -> dict[str, Any]:
    await command(
        client,
        "Emulation.setDeviceMetricsOverride",
        {
            "width": case.width,
            "height": case.height,
            "deviceScaleFactor": device_scale_factor,
            "mobile": False,
        },
        session_id=session_id,
    )
    data_url = "data:text/html," + urllib.parse.quote(case.html)
    _, navigation_messages = await command(
        client,
        "Page.navigate",
        {"url": data_url},
        session_id=session_id,
    )
    await wait_for_load(client, session_id, navigation_messages)

    expression = """(() => {
      const ids = %s;
      const rects = Object.fromEntries(ids.map(id => {
        const r = document.getElementById(id).getBoundingClientRect();
        return [id, [r.x, r.y, r.width, r.height]];
      }));
      return {rects, viewport: [innerWidth, innerHeight], documentSize: [document.documentElement.scrollWidth, document.documentElement.scrollHeight]};
    })()""" % json.dumps(list(case.expected_rects))
    result, _ = await command(
        client,
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True},
        session_id=session_id,
    )
    exception = result.get("exceptionDetails")
    if exception is not None:
        raise RuntimeError(f"{case.name}: Runtime.evaluate failed: {exception}")
    value = result["result"]["value"]
    if value["viewport"] != [case.width, case.height]:
        raise AssertionError(
            f"{case.name}.viewport: expected {[case.width, case.height]}, got {value['viewport']}"
        )
    for key, expected in case.expected_rects.items():
        assert_close(case.name, key, value["rects"][key], expected)
    if case.expected_document_size is not None:
        assert_close(
            case.name,
            "documentSize",
            value["documentSize"],
            case.expected_document_size,
        )
    return value


async def run(binary: Path) -> dict[str, Any]:
    port = reserve_port()
    endpoint = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(prefix="moli-layout-chromium-") as profile:
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
                except BaseException as error:  # startup evidence retains the last error
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

            measured = {
                case.name: await measure_case(client, session_id, case) for case in CASES
            }
            dpr_one = await measure_case(client, session_id, CASES[1], device_scale_factor=1.0)
            dpr_two = await measure_case(client, session_id, CASES[1], device_scale_factor=2.0)
            if dpr_one["rects"] != dpr_two["rects"]:
                raise AssertionError("deviceScaleFactor changed CSS layout coordinates")
            return {
                "status": "passed",
                "product": version.get("product"),
                "revision": version.get("revision"),
                "tolerance_css_px": TOLERANCE,
                "cases": measured,
                "dpr_invariant": True,
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", type=Path, default=DEFAULT_CHROMIUM)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    binary = args.chromium.expanduser().resolve()
    if not binary.is_file():
        raise SystemExit(f"Chromium binary does not exist: {binary}")
    print(json.dumps(asyncio.run(run(binary)), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
